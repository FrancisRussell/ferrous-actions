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
use chrono::{DateTime, Utc};
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

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
struct GroupIdentifier {
    root: String,
    path: String,
    num_entries: usize,
    entries_hash: HashValue,
}

impl Cache {
    pub async fn new(cache_type: CacheType) -> Result<Cache, Error> {
        // Delete derived content at any paths we want to build the cache at
        for delete_path in find_additional_delete_paths(cache_type).await? {
            if delete_path.exists().await {
                info!("Pruning redundant cache element: {}", delete_path);
                actions::io::rm_rf(&delete_path).await?;
            }
        }
        let grouping_depth = cache_type.grouping_depth();
        let entry_depth = cache_type.entry_depth();
        assert!(
            grouping_depth <= entry_depth,
            "Cannot group at a higher depth than individual cache entries"
        );
        let top_depth_glob = depth_to_match(grouping_depth)?;
        let folder_path = find_path(cache_type);
        let top_depth_paths = match_relative_paths(&folder_path, &top_depth_glob, true).await?;
        let entry_depth_relative = entry_depth - grouping_depth;
        let mut map = BTreeMap::new();
        for group in top_depth_paths {
            let group_path = folder_path.join(group.clone());
            map.insert(
                group.to_string(),
                Self::build_group(cache_type, &group_path, entry_depth_relative).await?,
            );
        }
        Ok(Cache {
            cache_type,
            root: map,
            root_path: folder_path.to_string(),
        })
    }

    fn build_group_identifier(&self, group_path: &str) -> GroupIdentifier {
        let group = &self
            .root
            .get(group_path)
            .unwrap_or_else(|| panic!("Unknown group: {}", group_path));
        let mut hasher = Blake3Hasher::default();
        group.len().hash(&mut hasher);
        group.keys().for_each(|k| k.hash(&mut hasher));
        GroupIdentifier {
            root: self.root_path.clone(),
            path: group_path.to_string(),
            num_entries: group.len(),
            entries_hash: hasher.hash_value(),
        }
    }

    pub async fn restore_from_env(cache_type: CacheType, scope: &HashValue) -> Result<Cache, Error> {
        use crate::access_times::revert_folder;
        use itertools::Itertools as _;

        let job = Job::from_env()?;

        // Delete existing cache
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

        let entry = build_cache_entry_dependencies(cache_type, scope, &job)?;
        let restore_key = entry.restore().await.map_err(Error::Js)?;
        if let Some(restore_key) = restore_key {
            info!(
                "Located dependencies list for {} in cache using key {}.",
                cache_type.friendly_name(),
                restore_key
            );
            let dep_file_path = dependency_file_path(cache_type, scope, &job)?;
            let groups: Vec<GroupIdentifier> = {
                let file_contents = node::fs::read_file(&dep_file_path).await?;
                serde_json::de::from_slice(&file_contents)?
            };
            let group_list_string = groups.iter().map(|g| &g.path).join(", ");
            info!(
                "The following groups will be restored for cache type {}: {}",
                cache_type.friendly_name(),
                group_list_string
            );
            for group in &groups {
                let entry = Self::group_identifier_to_cache_entry(cache_type, group);
                if let Some(name) = entry.restore().await? {
                    info!("Restored cache key: {}", name);
                } else {
                    info!(
                        "Failed to find {} cache entry for {}",
                        cache_type.friendly_name(),
                        group.path
                    );
                }
            }
        } else {
            info!("No existing dependency list for {} found.", cache_type.friendly_name());
        }
        // Ensure we at least have an empty folder
        node::fs::create_dir_all(&folder_path).await?;
        // Revert access times
        revert_folder(&folder_path).await?;
        Self::new(cache_type).await
    }

    pub async fn save_changes(
        &self,
        old: &Cache,
        scope_hash: &HashValue,
        min_recache_interval: &chrono::Duration,
    ) -> Result<(), Error> {
        let job = Job::from_env()?;
        let dep_file_path = dependency_file_path(self.cache_type, &scope_hash, &job)?;
        let old_groups = if dep_file_path.exists().await {
            let file_contents = node::fs::read_file(&dep_file_path).await?;
            let old_groups = serde_json::de::from_slice(&file_contents)?;
            Some(old_groups)
        } else {
            None
        };
        let new_groups = self.group_identifiers();
        if old_groups.as_ref() != Some(&new_groups) {
            info!(
                "Saving dependency list changes for cache of type {}",
                self.cache_type.friendly_name()
            );
            let serialized_groups = serde_json::to_string(&new_groups)?;
            {
                let parent = dep_file_path.parent();
                node::fs::create_dir_all(&parent).await?;
            }
            node::fs::write_file(&dep_file_path, serialized_groups.as_bytes()).await?;
            let dependencies_entry = build_cache_entry_dependencies(self.cache_type, &scope_hash, &job)?;
            dependencies_entry.save().await?;
        } else {
            info!(
                "No changes in depdendency list for cache of type {}",
                self.cache_type.friendly_name()
            );
        }

        for (path, group) in &self.root {
            let attempt_save = if let Some(old_group) = old.root.get(path) {
                if Self::groups_identical(group, old_group) {
                    // The group's content is unchanged
                    false
                } else {
                    // The modification time is dubious because we cannot track when file deletions
                    // occur and modifications times could be preserved from some sort of archive.
                    // It should work fine for changes to Git repos however, which are our main
                    // concern.
                    let old_modification = Self::group_last_modified(old_group).unwrap_or_default();
                    // Be robust against our delta being negative.
                    let modification_delta = chrono::Utc::now() - old_modification;
                    let modification_delta = std::cmp::max(chrono::Duration::zero(), modification_delta);

                    let interval_is_sufficient = modification_delta > *min_recache_interval;
                    if interval_is_sufficient {
                        //let delta =
                        // folder_info_new.fingerprint.changes_from(&folder_info_old.fingerprint);
                        // info!("{}", render_delta_items(&delta));
                        true
                    } else {
                        use humantime::format_duration;
                        info!(
                            "Cached {} outdated by {}, but not updating cache since minimum recache interval is {}.",
                            self.cache_type,
                            format_duration(modification_delta.to_std()?),
                            format_duration(min_recache_interval.to_std()?),
                        );
                        false
                    }
                }
            } else {
                // The group did not previously exist in the cache
                true
            };

            if attempt_save {
                let identifier = self.build_group_identifier(path);
                let entry = Self::group_identifier_to_cache_entry(self.cache_type, &identifier);
                info!(
                    "Saving modified content for cache type {} for {}",
                    self.cache_type.friendly_name(),
                    path
                );
                entry.save().await?;
            }
        }
        Ok(())
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

    fn group_identifiers(&self) -> Vec<GroupIdentifier> {
        self.root
            .keys()
            .map(|group_path| self.build_group_identifier(group_path))
            .collect()
    }

    fn group_identifier_to_cache_entry(cache_type: CacheType, group_id: &GroupIdentifier) -> CacheEntry {
        use crate::cache_key_builder::CacheKeyBuilder;

        let name = format!("{} (content)", cache_type.friendly_name());
        let date = chrono::Local::now();
        let mut builder = CacheKeyBuilder::new(&name);
        builder.add_key_data(group_id);
        builder.set_attribute("path", group_id.path.as_str());
        builder.set_attribute("num_entries", &group_id.num_entries.to_string());
        builder.set_attribute("date", &date.to_string());
        builder.set_attribute_nonce("nonce");
        let mut entry = builder.into_entry();
        let path = Path::from(group_id.root.as_str()).join(group_id.path.as_str());
        entry.path(path);
        entry
    }

    fn group_last_modified(group: &BTreeMap<String, Fingerprint>) -> Option<DateTime<Utc>> {
        let mut result = None;
        for fingerprint in group.values() {
            result = match (result, fingerprint.modified()) {
                (None, modified) => modified,
                (modified, None) => modified,
                (Some(a), Some(b)) => Some(std::cmp::max(a, b)),
            };
        }
        result
    }

    fn groups_identical(from: &BTreeMap<String, Fingerprint>, to: &BTreeMap<String, Fingerprint>) -> bool {
        use itertools::{EitherOrBoth, Itertools as _};
        let from_iter = from.iter();
        let to_iter = to.iter();
        let merged = from_iter.merge_join_by(to_iter, |left, right| left.0.cmp(&right.0));
        for element in merged {
            match element {
                EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => return false,
                EitherOrBoth::Both(left, right) => {
                    if left.1.content_hash() != right.1.content_hash() {
                        return false;
                    }
                }
            }
        }
        true
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
    info!("Job: {:#?}", job);
    let cached_types = get_types_to_cache(input_manager)?;
    for cache_type in cached_types {
        // Mark as used to avoid spurious warnings (we only use this when we save the
        // entries)
        let _ = get_min_recache_interval(input_manager, cache_type)?;

        // Build the cache
        let cache = Cache::restore_from_env(cache_type, &scope_hash).await?;
        let serialized_cache = serde_json::to_string(&cache)?;
        let cached_info_path = cached_folder_info_path(cache_type)?;
        {
            let parent = cached_info_path.parent();
            node::fs::create_dir_all(&parent).await?;
        }
        node::fs::write_file(&cached_info_path, serialized_cache.as_bytes()).await?;
    }
    Ok(())
}

pub async fn save_cargo_cache(input_manager: &input_manager::Manager) -> Result<(), Error> {
    let scope_hash = core::get_state(SCOPE_HASH_KEY).expect("Failed to find scope ID hash");
    let scope_hash = safe_encoding::decode(&scope_hash).expect("Failed to decode scope ID hash");
    let scope_hash = HashValue::from_bytes(&scope_hash);

    let atimes_supported = core::get_state(ATIMES_SUPPORTED_KEY).expect("Failed to find access times support flag");
    let atimes_supported: bool = serde_json::de::from_str(&atimes_supported)?;

    let job = Job::from_env()?;
    let cached_types = get_types_to_cache(input_manager)?;

    info!("Job: {:#?}", job);

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
        let mut cache = Cache::new(cache_type).await?;

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

        // Save groups to cache if they have changed
        let min_recache_interval = get_min_recache_interval(input_manager, cache_type)?;
        cache
            .save_changes(&cache_old, &scope_hash, &min_recache_interval)
            .await?;
    }
    Ok(())
}
