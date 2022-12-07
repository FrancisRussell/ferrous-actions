use crate::fingerprinting::fingerprint_directory;
use crate::info;
use crate::node;
use crate::node::os::homedir;
use crate::node::path::Path;
use crate::Error;
use serde::{Deserialize, Serialize};

fn find_cargo_home() -> Path {
    let mut path = homedir();
    path.push(".cargo");
    path
}

fn find_index_path() -> Path {
    let mut path = find_cargo_home();
    path.push("registry");
    path.push("index");
    path
}

fn cached_folder_info_path(cache_type: CacheType) -> Path {
    let mut dir = node::os::homedir();
    dir.push(".cache");
    dir.push("github-rust-actions");
    dir.push("cached_folder_info");
    let file_name = {
        let stem = match cache_type {
            CacheType::Index => "index",
        };
        format!("{}.toml", stem)
    };
    dir.push(file_name.as_str());
    dir
}

#[derive(Debug, Clone, Copy)]
enum CacheType {
    Index,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedFolderInfo {
    path: String,
    fingerprint: u64,
}

async fn build_cached_folder_info(path: &Path) -> Result<CachedFolderInfo, Error> {
    let fingerprint = fingerprint_directory(&path).await?;
    let folder_info = CachedFolderInfo {
        path: path.to_string(),
        fingerprint,
    };
    Ok(folder_info)
}

pub async fn restore_cargo_cache() -> Result<(), Error> {
    // TODO: Delete index if it already exists (and probably warn)
    // TODO: Attempt index restore
    // TODO: If restore failed, make sure we have an empty folder
    let index_path = find_index_path();
    let folder_info = build_cached_folder_info(&index_path).await?;
    let folder_info_serialized = serde_json::to_string_pretty(&folder_info)?;
    info!("Index info: {}", folder_info_serialized);
    let folder_info_path = cached_folder_info_path(CacheType::Index);
    let parent = folder_info_path.parent();
    node::fs::create_dir_all(&parent).await?;
    node::fs::write_file(&folder_info_path, folder_info_serialized.as_bytes()).await?;
    Ok(())
}

pub async fn save_cargo_cache() -> Result<(), Error> {
    use wasm_bindgen::JsError;

    let index_path = find_index_path();
    let folder_info_new = build_cached_folder_info(&index_path).await?;
    let folder_info_old: CachedFolderInfo = {
        let folder_info_path = cached_folder_info_path(CacheType::Index);
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
        info!("{} unchanged, no need to write to cache", index_path);
    } else {
        info!(
            "{} fingerprint changed from {} to {}",
            index_path, folder_info_old.fingerprint, folder_info_new.fingerprint
        );
    }
    Ok(())
}
