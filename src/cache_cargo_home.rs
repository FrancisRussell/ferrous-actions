use crate::actions;
use crate::actions::cache::CacheEntry;
use crate::fingerprinting::fingerprint_directory_with_ignores;
use crate::fingerprinting::Ignores;
use crate::node;
use crate::node::os::homedir;
use crate::node::path::Path;
use crate::Error;
use crate::{error, info, warning};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

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

fn cached_folder_info_path(cache_type: CacheType) -> Path {
    let mut dir = node::os::homedir();
    dir.push(".cache");
    dir.push("github-rust-actions");
    dir.push("cached_folder_info");
    let file_name = format!("{}.toml", cache_type.short_name());
    dir.push(file_name.as_str());
    dir
}

#[derive(Debug, Clone, Copy)]
enum CacheType {
    Index,
    Crates,
}

impl CacheType {
    fn short_name(&self) -> Cow<str> {
        match *self {
            CacheType::Index => "index",
            CacheType::Crates => "crates",
        }
        .into()
    }

    fn friendly_name(&self) -> Cow<str> {
        match *self {
            CacheType::Index => "Registry index",
            CacheType::Crates => "Crate files",
        }
        .into()
    }

    fn relative_path(&self) -> Path {
        match *self {
            CacheType::Index => {
                let mut path = Path::from("registry");
                path.push("index");
                path
            }
            CacheType::Crates => {
                let mut path = Path::from("registry");
                path.push("cache");
                path
            }
        }
    }

    fn ignores(&self) -> Ignores {
        let mut ignores = Ignores::default();
        match *self {
            CacheType::Index => {
                ignores.add(1, ".last-updated");
            }
            CacheType::Crates => {}
        }
        ignores
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedFolderInfo {
    path: String,
    fingerprint: u64,
}

async fn build_cached_folder_info(cache_type: CacheType) -> Result<CachedFolderInfo, Error> {
    let path = find_path(cache_type);
    let ignores = cache_type.ignores();
    let fingerprint = fingerprint_directory_with_ignores(&path, &ignores).await?;
    let folder_info = CachedFolderInfo {
        path: path.to_string(),
        fingerprint,
    };
    Ok(folder_info)
}

fn build_cache_entry(cache_type: CacheType, path: &Path) -> CacheEntry {
    use crate::nonce::build_nonce;
    let nonce = build_nonce(8);
    let nonce = base64::encode_config(&nonce, base64::URL_SAFE);
    let name = cache_type.friendly_name();

    let primary_key = format!("{} - {}", name, nonce);
    let mut cache_entry = CacheEntry::new(primary_key.as_str());
    let secondary_key = format!("{}", name);
    cache_entry.restore_key(secondary_key.as_str());
    cache_entry.path(path);
    cache_entry
}

pub async fn restore_cargo_cache() -> Result<(), Error> {
    for cache_type in [CacheType::Index, CacheType::Crates].into_iter() {
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
        if cache_entry.restore().await.map_err(Error::Js)?.is_some() {
            info!("Restored {} from cache.", cache_type.friendly_name());
        } else {
            info!(
                "No existing cache entry for {} found.",
                cache_type.friendly_name()
            );
            node::fs::create_dir_all(&folder_path).await?;
        }
        let folder_info = build_cached_folder_info(cache_type).await?;
        let folder_info_serialized = serde_json::to_string_pretty(&folder_info)?;
        let folder_info_path = cached_folder_info_path(cache_type);
        let parent = folder_info_path.parent();
        node::fs::create_dir_all(&parent).await?;
        node::fs::write_file(&folder_info_path, folder_info_serialized.as_bytes()).await?;
    }
    Ok(())
}

pub async fn save_cargo_cache() -> Result<(), Error> {
    use wasm_bindgen::JsError;

    for cache_type in [CacheType::Index, CacheType::Crates].into_iter() {
        let folder_path = find_path(cache_type);
        let folder_info_new = build_cached_folder_info(cache_type).await?;
        let folder_info_old: CachedFolderInfo = {
            let folder_info_path = cached_folder_info_path(cache_type);
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
            let cache_entry = build_cache_entry(cache_type, &folder_path);
            if let Err(e) = cache_entry.save().await.map_err(Error::Js) {
                error!(
                    "Failed to save {} to cache: {}",
                    cache_type.friendly_name(),
                    e
                );
            } else {
                info!("Saved {} to cache.", cache_type.friendly_name());
            }
        }
    }
    Ok(())
}
