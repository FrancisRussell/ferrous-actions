use crate::action_paths::get_action_cache_dir;
use crate::actions::cache::Entry as CacheEntry;
use crate::actions::core;
use crate::dir_tree::match_relative_paths;
use crate::fingerprinting::{fingerprint_path_with_ignores, render_delta_items, Fingerprint, Ignores};
use crate::hasher::Blake3 as Blake3Hasher;
use crate::input_manager::{self, Input};
use crate::job::Job;
use crate::node::os::homedir;
use crate::node::path::Path;
use crate::{actions, error, info, node, notice, safe_encoding, warning, Error};
use rust_toolchain_manifest::HashValue;
use serde::{Deserialize, Serialize};
use simple_path_match::{PathMatch, PathMatchBuilder};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::hash::{Hash as _, Hasher as _};
use std::str::FromStr;
use strum::{Display, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

const SCOPE_HASH_KEY: &str = "SCOPE_HASH";
const ATIMES_SUPPORTED_KEY: &str = "ACCESS_TIMES_SUPPORTED";
const CACHED_TYPES_KEY: &str = "CACHED_TYPES";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Cache {
    cache_type: CacheType,
    root: BTreeMap<String, BTreeMap<String, Fingerprint>>,
    root_path: String,
}

#[derive(Clone, Debug)]
struct CacheGroupKey {
    save: String,
    restore: String,
}

impl Cache {
    pub async fn new(cache_type: CacheType, path: &Path) -> Result<Cache, Error> {
        let grouping_depth = cache_type.grouping_depth();
        let entry_depth = cache_type.entry_depth();
        assert!(
            grouping_depth <= entry_depth,
            "Cannot group at a higher level than individual cache entries"
        );
        let top_depth_glob = depth_to_match(grouping_depth)?;
        let top_depth_paths = match_relative_paths(path, &top_depth_glob, true).await?;
        let entry_depth_relative = entry_depth - grouping_depth;
        let mut map = BTreeMap::new();
        for group in top_depth_paths {
            let group_path = path.join(group.clone());
            map.insert(
                group.to_string(),
                Self::build_group(cache_type, &group_path, entry_depth_relative).await?,
            );
        }
        Ok(Cache {
            cache_type,
            root: map,
            root_path: path.to_string(),
        })
    }

    async fn build_entry(cache_type: CacheType, entry_path: &Path) -> Result<Fingerprint, Error> {
        let ignores = cache_type.ignores();
        fingerprint_path_with_ignores(entry_path, &ignores).await
    }

    async fn build_group(
        cache_type: CacheType,
        group_path: &Path,
        entry_level: usize,
    ) -> Result<BTreeMap<String, Fingerprint>, Error> {
        let entry_level_glob = depth_to_match(entry_level)?;
        let entry_level_paths = match_relative_paths(&group_path, &entry_level_glob, true).await?;
        let mut map = BTreeMap::new();
        for path in entry_level_paths {
            let entry_path = group_path.join(path.clone());
            map.insert(path.to_string(), Self::build_entry(cache_type, &entry_path).await?);
        }
        Ok(map)
    }

    pub fn group_cache_keys(&self) -> Vec<CacheGroupKey> {
        use crate::cache_key_builder::CacheKeyBuilder;

        let date = chrono::Local::now();
        let mut result = Vec::with_capacity(self.root.len());
        for (group_path, group) in &self.root {
            let name = format!("{} (content)", self.cache_type.friendly_name());
            let mut builder = CacheKeyBuilder::new(&name);
            builder.add_key_data(group_path);
            builder.set_attribute("path", group_path);
            builder.set_attribute("num_entries", &group.len().to_string());
            builder.set_attribute("date", &date.to_string());
            builder.set_attribute_nonce("nonce");
            for entry_path in group.keys() {
                builder.add_key_data(group_path);
            }
            let group_key = CacheGroupKey {
                save: builder.current_save_key(),
                restore: builder.current_restore_key(),
            };
            result.push(group_key);
        }
        result
    }

    async fn prune_unused_group(
        left: &BTreeMap<String, Fingerprint>,
        right: &mut BTreeMap<String, Fingerprint>,
        right_path: &Path,
    ) -> Result<(), Error> {
        use itertools::{EitherOrBoth, Itertools as _};
        let mut to_prune = HashSet::new();
        let from_iter = left.iter();
        let to_iter = right.iter();
        let merged = from_iter.merge_join_by(to_iter, |left, right| left.0.cmp(&right.0));
        for element in merged {
            match element {
                EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => {}
                EitherOrBoth::Both(left, right) => {
                    if left.1.accessed() == right.1.accessed() {
                        to_prune.insert(right.0.to_string());
                    }
                }
            }
        }
        for element_name in to_prune {
            let path = right_path.join(element_name.as_str());
            info!("Pruning unused cache element at {}", path);
            actions::io::rm_rf(&path).await?;
            right.remove(&element_name);
        }
        Ok(())
    }

    pub async fn prune_unused(&mut self, old: &Cache) -> Result<(), Error> {
        use itertools::{EitherOrBoth, Itertools as _};
        let root_path = Path::from(self.root_path.as_str());
        let from_iter = old.root.iter();
        let to_iter = self.root.iter_mut();
        let merged = from_iter.merge_join_by(to_iter, |left, right| left.0.cmp(&right.0));
        for element in merged {
            match element {
                EitherOrBoth::Left(_) => {}
                EitherOrBoth::Right(_) => {}
                EitherOrBoth::Both(left, mut right) => {
                    let entry_path = root_path.join(right.0.as_str());
                    Self::prune_unused_group(&left.1, &mut right.1, &entry_path).await?;
                }
            }
        }
        self.root.retain(|k, v| {
            let keep = !v.is_empty();
            if !keep {
                info!("Removing empty cache group: {}", k);
            }
            keep
        });
        Ok(())
    }

    pub fn get_root_path(&self) -> Path {
        Path::from(self.root_path.as_str())
    }
}

fn find_cargo_home() -> Path {
    homedir().join(".cargo")
}

fn find_path(cache_type: CacheType) -> Path {
    find_cargo_home().join(cache_type.relative_path())
}

fn depth_to_match(depth: usize) -> Result<PathMatch, Error> {
    use itertools::Itertools as _;

    let pattern = if depth == 0 {
        ".".into()
    } else {
        std::iter::repeat("*").take(depth).join("/")
    };
    Ok(PathMatch::from_pattern(&pattern, &node::path::separator())?)
}

async fn find_additional_delete_paths(cache_type: CacheType) -> Result<Vec<Path>, Error> {
    let mut path_match_builder = PathMatchBuilder::new(&node::path::separator());
    cache_type.add_additional_delete_paths(&mut path_match_builder)?;
    let path_matcher = path_match_builder.build()?;
    let home_path = find_cargo_home();
    let result = if home_path.exists().await {
        match_relative_paths(&home_path, &path_matcher, false).await?
    } else {
        Vec::new()
    };
    Ok(result)
}

fn cached_folder_info_path(cache_type: CacheType) -> Result<Path, Error> {
    let file_name = format!("{}.json", cache_type.short_name());
    Ok(get_action_cache_dir()?
        .join("cached-folder-info")
        .join(file_name.as_str()))
}

fn dependency_files_dir() -> Result<Path, Error> {
    Ok(get_action_cache_dir()?.join("dependency-data"))
}

#[derive(
    Debug, Clone, Copy, EnumIter, EnumString, Eq, Hash, PartialEq, IntoStaticStr, Display, Serialize, Deserialize,
)]
enum CacheType {
    #[strum(serialize = "indices")]
    Indices,

    #[strum(serialize = "crates")]
    Crates,

    #[strum(serialize = "git-repos")]
    GitRepos,
}

impl CacheType {
    fn short_name(&self) -> Cow<str> {
        let name: &str = self.into();
        name.into()
    }

    fn friendly_name(&self) -> Cow<str> {
        match *self {
            CacheType::Indices => "registry indices",
            CacheType::Crates => "crate files",
            CacheType::GitRepos => "Git repositories",
        }
        .into()
    }

    fn relative_path(self) -> Path {
        match self {
            CacheType::Indices => Path::from("registry").join("index"),
            CacheType::Crates => Path::from("registry").join("cache"),
            CacheType::GitRepos => Path::from("git").join("db"),
        }
    }

    fn add_additional_delete_paths(self, match_builder: &mut PathMatchBuilder) -> Result<(), Error> {
        // These are paths we should delete at the same time as restoring the cache and
        // also before saving. This is primarily because we want to see what in
        // the cache is accessed, and leaving derived information about can
        // cause cached items to never have their content read, leading to items
        // repeatedly being evicted then restored.
        match self {
            CacheType::Indices => {
                match_builder.add_pattern("registry/index/*/.cache")?;
            }
            CacheType::Crates => {
                match_builder.add_pattern("registry/src")?;
            }
            CacheType::GitRepos => {
                match_builder.add_pattern("git/checkouts")?;
            }
        }
        Ok(())
    }

    fn ignores(self) -> Ignores {
        // Depths are relative to the entry
        let mut ignores = Ignores::default();
        match self {
            CacheType::Indices => {
                ignores.add(1, ".last-updated");
            }
            CacheType::Crates | CacheType::GitRepos => {}
        }
        ignores
    }

    fn grouping_depth(self) -> usize {
        1
    }

    fn entry_depth(self) -> usize {
        match self {
            CacheType::Indices | CacheType::GitRepos => 1,
            CacheType::Crates => {
                // This means we can prune individual crate files within an index
                2
            }
        }
    }

    fn default_min_recache_interval(self) -> chrono::Duration {
        match self {
            CacheType::Indices => chrono::Duration::days(1),
            _ => chrono::Duration::zero(),
        }
    }

    fn min_recache_input(self) -> input_manager::Input {
        match self {
            CacheType::Indices => input_manager::Input::MinRecacheIndices,
            CacheType::GitRepos => input_manager::Input::MinRecacheGitRepos,
            CacheType::Crates => input_manager::Input::MinRecacheCrates,
        }
    }
}

fn get_types_to_cache(input_manager: &input_manager::Manager) -> Result<Vec<CacheType>, Error> {
    let mut result = HashSet::new();
    if let Some(types) = input_manager.get(Input::CacheOnly) {
        let types = types.split_whitespace();
        for cache_type in types {
            let cache_type =
                CacheType::from_str(cache_type).map_err(|_| Error::ParseCacheableItem(cache_type.to_string()))?;
            result.insert(cache_type);
        }
    } else {
        result.extend(CacheType::iter());
    }
    Ok(result.into_iter().collect())
}

fn get_min_recache_interval(
    input_manager: &input_manager::Manager,
    cache_type: CacheType,
) -> Result<chrono::Duration, Error> {
    let result = if let Some(duration) = input_manager.get(cache_type.min_recache_input()) {
        let duration = humantime::parse_duration(duration)?;
        chrono::Duration::from_std(duration)?
    } else {
        cache_type.default_min_recache_interval()
    };
    Ok(result)
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedFolderInfo {
    path: String,
    fingerprint: Fingerprint,
}

async fn build_cached_folder_info(cache_type: CacheType) -> Result<CachedFolderInfo, Error> {
    let path = find_path(cache_type);
    let ignores = cache_type.ignores();
    let fingerprint = fingerprint_path_with_ignores(&path, &ignores).await?;
    let folder_info = CachedFolderInfo {
        path: path.to_string(),
        fingerprint,
    };
    Ok(folder_info)
}

fn dependency_file_path(cache_type: CacheType, scope: &HashValue, job: &Job) -> Result<Path, Error> {
    let dependency_dir = dependency_files_dir()?;
    let mut hasher = Blake3Hasher::default();
    scope.hash(&mut hasher);
    cache_type.hash(&mut hasher);
    job.hash(&mut hasher);
    let file_name = format!("{}.json", hasher.hash_value());
    Ok(dependency_dir.join(file_name.as_str()))
}

fn build_cache_entry_dependencies(cache_type: CacheType, scope: &HashValue, job: &Job) -> Result<CacheEntry, Error> {
    use crate::cache_key_builder::CacheKeyBuilder;
    let name = format!("{} (dependencies)", cache_type.friendly_name());
    let mut key_builder = CacheKeyBuilder::new(&name);
    key_builder.add_key_data(scope);
    key_builder.add_key_data(job);
    let date = chrono::Local::now();
    key_builder.set_attribute("date", &date.to_string());
    key_builder.set_attribute_nonce("nonce");
    key_builder.set_attribute("workflow", job.get_workflow());
    key_builder.set_attribute("job", job.get_job_id());
    if let Some(properties) = job.matrix_properties_as_string() {
        key_builder.set_attribute("matrix", &properties);
    }
    let mut cache_entry = key_builder.into_entry();
    let path = dependency_file_path(cache_type, scope, job)?;
    info!("Dependency file path: {}", path);
    cache_entry.path(path);
    Ok(cache_entry)
}

fn build_cache_entry(cache_type: CacheType, key: &HashValue, path: &Path) -> CacheEntry {
    use crate::cache_key_builder::CacheKeyBuilder;

    let name = cache_type.friendly_name();
    let mut key_builder = CacheKeyBuilder::new(&name);
    key_builder.add_key_data(key);
    let date = chrono::Local::now();
    key_builder.set_attribute("date", &date.to_string());
    key_builder.set_attribute_nonce("nonce");
    let mut cache_entry = key_builder.into_entry();
    cache_entry.path(path);
    cache_entry
}

pub async fn restore_cargo_cache(input_manager: &input_manager::Manager) -> Result<(), Error> {
    use crate::access_times::supports_atime;
    use crate::cargo_lock_hashing::hash_cargo_lock_files;

    info!("Checking to see if filesystem supports access times...");
    let atimes_supported = supports_atime().await?;
    if atimes_supported {
        info!(concat!(
            "File access times supported. Hooray! ",
            "These will be used to intelligently decide what can be dropped from within cached cargo home items."
        ));
    } else {
        notice!(concat!("File access times not supported - cannot perform intelligent cache pruning. ",
            "Likely this platform is Windows. ",
            "The hash of all Cargo.lock files found in this folder will be used as part of the key for cached cargo home entries. ",
            "This means certain caches will be rebuilt from scratch whenever a Cargo.lock file changes. ",
            "This is to avoid cache entries growing monotonically. ",
            "Note that enabling file access times on Windows is generally a bad idea since Microsoft never implemented relatime semantics.")
        );
    }
    core::save_state(ATIMES_SUPPORTED_KEY, serde_json::to_string(&atimes_supported)?);

    let scope_hash = if atimes_supported {
        // We can't use the empty array because it will encode to an empty string, which
        // doesn't play well with `save_state`.
        HashValue::from_bytes(&[42u8])
    } else {
        let cwd = node::process::cwd();
        let lock_hash = hash_cargo_lock_files(&cwd).await?;
        HashValue::from_bytes(&lock_hash.bytes)
    };
    core::save_state(SCOPE_HASH_KEY, safe_encoding::encode(&scope_hash));

    let job = Job::from_env()?;
    let cached_types = get_types_to_cache(input_manager)?;
    for cache_type in cached_types {
        // Mark as used to avoid spurious warnings (we only use this when we save the
        // entries)
        let _ = get_min_recache_interval(input_manager, cache_type)?;

        // Delete existing content at the path we want to restore cached items to
        // TODO: Re-enable this when we are certain fingerprinting of each cache entry
        // works
        /*
        let folder_path = find_path(cache_type);
        if folder_path.exists().await {
            warning!(
                concat!(
                    "Cache action will delete existing contents of {} and derived information. ",
                    "To avoid this warning, place this action earlier or delete this before running the action."
                ),
                folder_path
            );
            actions::io::rm_rf(&folder_path).await?;
        }
        */

        // Delete derived content at any paths we want to restore cached items to
        for delete_path in find_additional_delete_paths(cache_type).await? {
            if delete_path.exists().await {
                info!("Pruning redundant cache element: {}", delete_path);
                actions::io::rm_rf(&delete_path).await?;
            }
        }

        // Attempt to locate the list of cache items we want to restore
        let dependencies_entry = build_cache_entry_dependencies(cache_type, &scope_hash, &job)?;
        if let Some(restore_key) = dependencies_entry.restore().await.map_err(Error::Js)? {
            info!(
                "Located dependencies list for {} in cache using key {}.",
                cache_type.friendly_name(),
                restore_key
            );
            //TODO: We actually need to open the dependency list and try to
            // restore entries now
        } else {
            info!("No existing dependency list for {} found.", cache_type.friendly_name());
        }

        // Ensure we at least have an empty folder
        let folder_path = find_path(cache_type);
        node::fs::create_dir_all(&folder_path).await?;

        // Revert access times
        if atimes_supported {
            use crate::access_times::revert_folder;
            revert_folder(&folder_path).await?;
        }

        // Build the cache
        let cache = Cache::new(cache_type, &folder_path).await?;
        let serialized_cache = serde_json::to_string(&cache)?;
        let cached_info_path = cached_folder_info_path(cache_type)?;
        {
            let parent = cached_info_path.parent();
            node::fs::create_dir_all(&parent).await?;
        }
        node::fs::write_file(&cached_info_path, serialized_cache.as_bytes()).await?;
        info!("Cache keys: {:#?}", cache.group_cache_keys());

        /*
        let entry_matcher = depth_to_match(entry_depth)?;
        let entry_paths = match_relative_paths(&folder_path, &entry_matcher).await?;
        info!("Cache entries for {}: {:#?}", cache_type, entry_paths);
        */
    }

    /*
    use crate::access_times::{revert_folder, supports_atime};

    let types_to_cache = get_types_to_cache(input_manager)?;
    let types_to_cache_json = serde_json::to_string(&types_to_cache)?;

    core::save_state(CACHED_TYPES_KEY, safe_encoding::encode(&types_to_cache_json));

    for cache_type in get_types_to_cache(input_manager)? {
        let cache_entry = build_cache_entry(cache_type, &id_hash, &folder_path);
        if let Some(restore_key) = cache_entry.restore().await.map_err(Error::Js)? {
            info!(
                "Restored {} from cache using key {}.",
                cache_type.friendly_name(),
                restore_key
            );
        } else {
            info!("No existing cache entry for {} found.", cache_type.friendly_name());
            node::fs::create_dir_all(&folder_path).await?;
        }
        // Set all access times to be prior to modification times
        if atimes_supported {
            revert_folder(&folder_path).await?;
        }
        let folder_info = build_cached_folder_info(cache_type).await?;
        let folder_info_serialized = serde_json::to_string(&folder_info)?;
        let folder_info_path = cached_folder_info_path(cache_type)?;
        let parent = folder_info_path.parent();
        node::fs::create_dir_all(&parent).await?;
        node::fs::write_file(&folder_info_path, folder_info_serialized.as_bytes()).await?;
    }
    */
    Ok(())
}

pub async fn save_cargo_cache(input_manager: &input_manager::Manager) -> Result<(), Error> {
    let scope_hash = core::get_state(SCOPE_HASH_KEY).expect("Failed to find scope ID hash");
    let scope_hash = safe_encoding::decode(&scope_hash).expect("Failed to decode scope ID hash");
    let scope_hash = HashValue::from_bytes(&scope_hash);

    let atimes_supported = core::get_state(ATIMES_SUPPORTED_KEY).expect("Failed to find access times support flag");
    let atimes_supported: bool = serde_json::de::from_str(&atimes_supported)?;

    let cached_types = get_types_to_cache(input_manager)?;
    /*
    use humantime::format_duration;
    use wasm_bindgen::JsError;
    */

    for cache_type in cached_types {
        // Delete items that should never make it into the cache
        for delete_path in find_additional_delete_paths(cache_type).await? {
            if delete_path.exists().await {
                info!("Pruning redundant cache element: {}", delete_path);
                actions::io::rm_rf(&delete_path).await?;
            }
        }

        // Restore the old cache
        let cache_old: Cache = {
            let cached_info_path = cached_folder_info_path(cache_type)?;
            let cache_serialized = node::fs::read_file(&cached_info_path).await?;
            serde_json::de::from_slice(&cache_serialized)?
        };

        // Construct the new cache
        let folder_path = find_path(cache_type);
        let mut cache = Cache::new(cache_type, &folder_path).await?;

        // Check the path to the cached items hasn't changed
        if cache.get_root_path() != cache_old.get_root_path() {
            use wasm_bindgen::JsError;
            let error = JsError::new(&format!(
                "Path to cache changed from {} to {}. Perhaps CARGO_HOME changed?",
                cache_old.get_root_path(),
                cache.get_root_path()
            ));
            return Err(Error::Js(error.into()));
        }

        // Prune unused items (if we have access time suppport)
        if atimes_supported {
            cache.prune_unused(&cache_old).await?;
        }

        info!("New group cache keys: {:#?}", cache.group_cache_keys());

        /*
        let mut folder_info_new = build_cached_folder_info(cache_type).await?;
        let folder_info_old: CachedFolderInfo = {
            let folder_info_path = cached_folder_info_path(cache_type)?;
            let folder_info_serialized = node::fs::read_file(&folder_info_path).await?;
            serde_json::de::from_slice(&folder_info_serialized)?
        };

        // If we prune redundant or unused files, we need to rebuild
        let mut rebuild_fingerprint = false;

        // Identify unaccessed items and prune them
        let unaccessed = folder_info_new
            .fingerprint
            .get_unaccessed_since(&folder_info_old.fingerprint, cache_type.entry_depth());

        let do_prune = !unaccessed.is_empty();
        if do_prune {
            for relative_path in unaccessed {
                info!("Pruning unused cache element: {}", relative_path);
                let full_path = folder_path.join(relative_path);
                actions::io::rm_rf(&full_path).await?;
                rebuild_fingerprint = true;
            }
        }

        if rebuild_fingerprint {
            folder_info_new = build_cached_folder_info(cache_type).await?;
        }

        if folder_info_old.fingerprint.content_hash() == folder_info_new.fingerprint.content_hash() {
            info!("{} unchanged, no need to write to cache", folder_path);
        } else {
            info!(
                "{} fingerprint changed from {} to {}",
                folder_path,
                folder_info_old.fingerprint.content_hash(),
                folder_info_new.fingerprint.content_hash()
            );

            // If we have no files in the old fingerprint, we assume it was updated at the
            // epoch. If we have no files in the new fingerprint, we assume it
            // was updated now. If we pruned items we also override the modification
            // timestamp with the current time.
            //
            // Basically, when we do not have a modification
            // time, we will return a delta that indicates the folder is
            // significantly out of date. Otherwise, we might end up not storing a
            // new cache entry when we create or fully empty a folder.
            let old_fingerprint = folder_info_old.fingerprint.modified().unwrap_or_default();
            let new_fingerprint = folder_info_new
                .fingerprint
                .modified()
                .filter(|_| !do_prune)
                .unwrap_or_else(chrono::Utc::now);

            // Be more robust against our file modification time moving backwards.
            let modification_delta = new_fingerprint - old_fingerprint;
            let modification_delta = std::cmp::max(chrono::Duration::zero(), modification_delta);

            let min_recache_interval = get_min_recache_interval(input_manager, cache_type)?;
            let interval_is_sufficient = modification_delta > min_recache_interval;
            if interval_is_sufficient {
                let delta = folder_info_new.fingerprint.changes_from(&folder_info_old.fingerprint);
                info!("{}", render_delta_items(&delta));

                let cache_entry = build_cache_entry(cache_type, &id_hash, &folder_path);
                if let Err(e) = cache_entry.save().await.map_err(Error::Js) {
                    error!("Failed to save {} to cache: {}", cache_type.friendly_name(), e);
                } else {
                    info!("Saved {} to cache.", cache_type.friendly_name());
                }
            } else {
                info!(
                    "Cached {} outdated by {}, but not updating cache since minimum recache interval is {}.",
                    cache_type,
                    format_duration(modification_delta.to_std()?),
                    format_duration(min_recache_interval.to_std()?),
                );
            }
        }
        */
    }
    Ok(())
}
