use crate::node::fs;
use crate::node::path::Path;
use async_recursion::async_recursion;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::Hash;
use std::hash::Hasher;
use wasm_bindgen::JsValue;

#[async_recursion(?Send)]
pub async fn fingerprint_directory(path: &Path) -> Result<u64, JsValue> {
    let dir = fs::read_dir(path).await?;
    let mut map = BTreeMap::new();
    for entry in dir {
        let path = entry.path();
        let file_type = entry.file_type();
        let hash = if file_type.is_dir() {
            fingerprint_directory(&path).await?
        } else {
            let stats = fs::symlink_metadata(&path).await?;
            let mut hasher = DefaultHasher::default();
            stats.mode().hash(&mut hasher);
            stats.uid().hash(&mut hasher);
            stats.gid().hash(&mut hasher);
            stats.len().hash(&mut hasher);
            stats.modified().hash(&mut hasher);
            hasher.finish()
        };
        map.insert(path.to_string(), hash);
    }
    let mut hasher = DefaultHasher::default();
    map.hash(&mut hasher);
    Ok(hasher.finish())
}
