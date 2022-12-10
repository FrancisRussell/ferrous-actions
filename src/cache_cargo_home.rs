use crate::action_paths::get_action_cache_dir;
use crate::actions::cache::CacheEntry;
use crate::actions::core;
use crate::fingerprinting::{fingerprint_directory_with_ignores, render_delta_items, Fingerprint, Ignores};
use crate::node::os::homedir;
use crate::node::path::Path;
use crate::{actions, error, info, node, notice, warning, Error};
use rust_toolchain_manifest::HashValue;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::str::FromStr;
use strum::{Display, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

const CARGO_LOCK_HASH_KEY: &str = "CARGO_LOCK_HASH";

fn find_cargo_home() -> Path {
    let mut path = homedir();
    path.push(".cargo");
    path
}

fn find_path(cache_type: CacheType) -> Path {
    let mut path = find_cargo_home();
    path.push(cache_type.relative_path());
    path
}

fn cached_folder_info_path(cache_type: CacheType) -> Result<Path, Error> {
    let mut dir = get_action_cache_dir()?;
    dir.push("cached_folder_info");
    let file_name = format!("{}.json", cache_type.short_name());
    dir.push(file_name.as_str());
    Ok(dir)
}

#[derive(Debug, Clone, Copy, EnumIter, EnumString, Eq, Hash, PartialEq, IntoStaticStr, Display)]
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
            CacheType::Indices => {
                let mut path = Path::from("registry");
                path.push("index");
                path
            }
            CacheType::Crates => {
                let mut path = Path::from("registry");
                path.push("cache");
                path
            }
            CacheType::GitRepos => {
                let mut path = Path::from("git");
                path.push("db");
                path
            }
        }
    }

    fn ignores(self) -> Ignores {
        let mut ignores = Ignores::default();
        match self {
            CacheType::Indices => {
                ignores.add(1, ".last-updated");
            }
            CacheType::Crates | CacheType::GitRepos => {}
        }
        ignores
    }

    fn default_min_recache_interval(self) -> chrono::Duration {
        match self {
            CacheType::Indices => chrono::Duration::days(1),
            _ => chrono::Duration::zero(),
        }
    }
}

fn get_types_to_cache() -> Result<Vec<CacheType>, Error> {
    let mut result = HashSet::new();
    if let Some(types) = actions::core::get_input("cache-only")? {
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

fn get_min_recache_interval(cache_type: CacheType) -> Result<chrono::Duration, Error> {
    let option_name = format!("min-recache-{}", cache_type);
    let result = if let Some(duration) = actions::core::get_input(option_name.as_str())? {
        let duration = humantime::parse_duration(duration.as_str())?;
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
    newly_created: bool,
}

async fn build_cached_folder_info(cache_type: CacheType) -> Result<CachedFolderInfo, Error> {
    let path = find_path(cache_type);
    let ignores = cache_type.ignores();
    let fingerprint = fingerprint_directory_with_ignores(&path, &ignores).await?;
    let folder_info = CachedFolderInfo {
        path: path.to_string(),
        fingerprint,
        newly_created: false,
    };
    Ok(folder_info)
}

fn build_cache_entry(cache_type: CacheType, key: &HashValue, path: &Path) -> CacheEntry {
    use crate::cache_key_builder::CacheKeyBuilder;
    use crate::date;

    let name = cache_type.friendly_name();
    let mut key_builder = CacheKeyBuilder::new(&name);
    key_builder.add_id_bytes(key.as_ref());
    let date = date::now();
    key_builder.set_attribute("date", &date.to_string());
    key_builder.set_attribute_nonce("nonce");
    let mut cache_entry = key_builder.into_entry();
    cache_entry.path(path);
    cache_entry
}

pub async fn restore_cargo_cache() -> Result<(), Error> {
    use crate::cargo_lock_hashing::hash_cargo_lock_files;
    use crate::check_atime_support::supports_atime;

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

    let cwd = node::process::cwd();
    let lock_hash = hash_cargo_lock_files(&cwd).await?;
    let lock_hash = HashValue::from_bytes(&lock_hash.bytes);
    core::save_state(
        CARGO_LOCK_HASH_KEY,
        base64::encode_config(lock_hash.as_ref(), base64::URL_SAFE),
    );

    for cache_type in get_types_to_cache()? {
        let folder_path = find_path(cache_type);
        if folder_path.exists().await {
            warning!(
                concat!(
                    "Cache action will delete existing contents of {}. ",
                    "To avoid this warning, place this action earlier or delete this before running the action."
                ),
                folder_path
            );
            actions::io::rm_rf(&folder_path).await?;
        }
        let cache_entry = build_cache_entry(cache_type, &lock_hash, &folder_path);
        let newly_created = if let Some(restore_key) = cache_entry.restore().await.map_err(Error::Js)? {
            info!(
                "Restored {} from cache using key {}.",
                cache_type.friendly_name(),
                restore_key
            );
            false
        } else {
            info!("No existing cache entry for {} found.", cache_type.friendly_name());
            node::fs::create_dir_all(&folder_path).await?;
            true
        };
        let mut folder_info = build_cached_folder_info(cache_type).await?;
        folder_info.newly_created = newly_created;
        let folder_info_serialized = serde_json::to_string(&folder_info)?;
        let folder_info_path = cached_folder_info_path(cache_type)?;
        let parent = folder_info_path.parent();
        node::fs::create_dir_all(&parent).await?;
        node::fs::write_file(&folder_info_path, folder_info_serialized.as_bytes()).await?;
    }
    Ok(())
}

pub async fn save_cargo_cache() -> Result<(), Error> {
    use humantime::format_duration;
    use wasm_bindgen::JsError;

    let lock_hash = core::get_state(CARGO_LOCK_HASH_KEY).expect("Failed to find Cargo.lock hash");
    let lock_hash = base64::decode_config(&lock_hash, base64::URL_SAFE).expect("Failed to decode Cargo.lock hash");
    let lock_hash = HashValue::from_bytes(&lock_hash);

    for cache_type in get_types_to_cache()? {
        let folder_path = find_path(cache_type);
        let folder_info_new = build_cached_folder_info(cache_type).await?;
        let folder_info_old: CachedFolderInfo = {
            let folder_info_path = cached_folder_info_path(cache_type)?;
            let folder_info_serialized = node::fs::read_file(&folder_info_path).await?;
            serde_json::de::from_slice(&folder_info_serialized)?
        };
        if folder_info_old.path != folder_info_new.path {
            let error = JsError::new(&format!(
                "Path to cache changed from {} to {}. Perhaps CARGO_HOME changed?",
                folder_info_old.path, folder_info_new.path
            ));
            return Err(Error::Js(error.into()));
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
            let modification_delta = folder_info_new.fingerprint.modified() - folder_info_old.fingerprint.modified();
            let min_recache_interval = get_min_recache_interval(cache_type)?;
            let interval_is_sufficient = modification_delta > min_recache_interval;
            if interval_is_sufficient {
                let delta = folder_info_new.fingerprint.changes_from(&folder_info_old.fingerprint);
                info!("{}", render_delta_items(&delta));
            }
            if folder_info_old.newly_created || interval_is_sufficient {
                let cache_entry = build_cache_entry(cache_type, &lock_hash, &folder_path);
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
    }
    Ok(())
}
