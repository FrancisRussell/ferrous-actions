use crate::node::fs;
use crate::node::path::Path;
use async_recursion::async_recursion;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::hash::Hasher;
use wasm_bindgen::JsValue;

#[derive(Debug, Default, Clone)]
pub struct Ignores {
    map: HashMap<usize, HashSet<String>>,
}

impl Ignores {
    pub fn add(&mut self, depth: usize, name: &str) {
        self.map.entry(depth).or_default().insert(name.to_string());
    }

    pub fn should_ignore(&self, name: &str, depth: usize) -> bool {
        if let Some(names) = self.map.get(&depth) {
            names.contains(name)
        } else {
            false
        }
    }
}

pub async fn fingerprint_directory(path: &Path) -> Result<u64, JsValue> {
    let ignores = Ignores::default();
    fingerprint_directory_with_ignores(path, &ignores).await
}

pub async fn fingerprint_directory_with_ignores(
    path: &Path,
    ignores: &Ignores,
) -> Result<u64, JsValue> {
    fingerprint_directory_with_ignores_impl(0, path, ignores).await
}

#[async_recursion(?Send)]
pub async fn fingerprint_directory_with_ignores_impl(
    depth: usize,
    path: &Path,
    ignores: &Ignores,
) -> Result<u64, JsValue> {
    let dir = fs::read_dir(path).await?;
    let mut map = BTreeMap::new();
    for entry in dir {
        if ignores.should_ignore(entry.file_name().as_str(), depth) {
            continue;
        }
        let path = entry.path();
        let file_type = entry.file_type();
        let hash = if file_type.is_dir() {
            fingerprint_directory_with_ignores_impl(depth + 1, &path, ignores).await?
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
