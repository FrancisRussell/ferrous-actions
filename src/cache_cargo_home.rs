use crate::action_paths::get_action_cache_dir;
use crate::actions::cache::CacheEntry;
use crate::fingerprinting::{fingerprint_directory_with_ignores, Ignores};
use crate::node::os::homedir;
use crate::node::path::Path;
use crate::{actions, error, info, node, warning, Error};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::str::FromStr;
use strum::{Display, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

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
    let file_name = format!("{}.toml", cache_type.short_name());
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
            CacheType::Indices => "Registry indices",
            CacheType::Crates => "Crate files",
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
    fingerprint: u64,
    modified: DateTime<Utc>,
    newly_created: bool,
}

async fn build_cached_folder_info(cache_type: CacheType) -> Result<CachedFolderInfo, Error> {
    let path = find_path(cache_type);
    let ignores = cache_type.ignores();
    let fingerprint = fingerprint_directory_with_ignores(&path, &ignores).await?;
    let folder_info = CachedFolderInfo {
        path: path.to_string(),
        fingerprint: fingerprint.content_hash(),
        modified: fingerprint.modified(),
        newly_created: false,
    };
    Ok(folder_info)
}

fn build_cache_entry(cache_type: CacheType, path: &Path) -> CacheEntry {
    use crate::date;
    use crate::nonce::build_nonce;
    let nonce = build_nonce(8);
    let nonce = base64::encode_config(nonce, base64::URL_SAFE);
    let name = cache_type.friendly_name();

    let date = date::now();
    let primary_key = format!("{} ({}; {})", name, date, nonce);
    let mut cache_entry = CacheEntry::new(primary_key.as_str());
    let secondary_key = name.to_string();
    cache_entry.restore_key(secondary_key.as_str());
    cache_entry.path(path);
    cache_entry
}

pub async fn restore_cargo_cache() -> Result<(), Error> {
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
        let cache_entry = build_cache_entry(cache_type, &folder_path);
        let newly_created = if cache_entry.restore().await.map_err(Error::Js)?.is_some() {
            info!("Restored {} from cache.", cache_type.friendly_name());
            false
        } else {
            info!("No existing cache entry for {} found.", cache_type.friendly_name());
            node::fs::create_dir_all(&folder_path).await?;
            true
        };
        let mut folder_info = build_cached_folder_info(cache_type).await?;
        folder_info.newly_created = newly_created;
        let folder_info_serialized = serde_json::to_string_pretty(&folder_info)?;
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
        if folder_info_old.fingerprint == folder_info_new.fingerprint {
            info!("{} unchanged, no need to write to cache", folder_path);
        } else {
            info!(
                "{} fingerprint changed from {} to {}",
                folder_path, folder_info_old.fingerprint, folder_info_new.fingerprint
            );
            let modification_delta = folder_info_new.modified - folder_info_old.modified;
            let min_recache_interval = get_min_recache_interval(cache_type)?;
            if folder_info_old.newly_created || modification_delta > min_recache_interval {
                let cache_entry = build_cache_entry(cache_type, &folder_path);
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
