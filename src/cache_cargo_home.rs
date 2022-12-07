use crate::node::path::Path;
use crate::node::os::homedir;
use crate::node;
use crate::Error;
use crate::info;
use crate::fingerprinting::fingerprint_directory;
use serde::{Serialize, Deserialize};

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

pub async fn restore_cargo_cache() -> Result<(), Error> {
    // TODO: Delete index if it already exists (and probably warn)
    // TODO: Attempt index restore
    // TODO: If restore failed, make sure we have an empty folder


    let index_path = find_index_path();
    let fingerprint = fingerprint_directory(&index_path).await?;
    let folder_info = CachedFolderInfo {
        path: index_path.to_string(),
        fingerprint,
    };
    let folder_info_serialized = serde_json::to_string_pretty(&folder_info)?;
    info!("Index info: {}", folder_info_serialized);
    let folder_info_path = cached_folder_info_path(CacheType::Index);
    let parent = folder_info_path.parent();
    node::fs::create_dir_all(&parent).await?;
    node::fs::write_file(&folder_info_path, folder_info_serialized.as_bytes()).await?;
    Ok(())
}
