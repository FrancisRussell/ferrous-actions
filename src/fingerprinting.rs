use crate::node::fs;
use crate::node::path::Path;
use async_recursion::async_recursion;
use chrono::{DateTime, Utc};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
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

#[derive(Debug, Clone)]
pub struct Metadata {
    uid: u64,
    gid: u64,
    len: u64,
    mode: u64,
    modified: DateTime<Utc>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum DeltaAction {
    Added,
    Removed,
    Changed,
}

impl From<&fs::Metadata> for Metadata {
    fn from(stats: &fs::Metadata) -> Metadata {
        Metadata {
            uid: stats.uid(),
            gid: stats.gid(),
            len: stats.len(),
            mode: stats.mode(),
            modified: stats.modified(),
        }
    }
}

impl Metadata {
    fn hash_noteworthy<H: Hasher>(&self, hasher: &mut H) {
        // Noteworthy basically means anything that would need an rsync
        self.uid.hash(hasher);
        self.gid.hash(hasher);
        self.len.hash(hasher);
        self.mode.hash(hasher);
        self.modified.hash(hasher);
    }

    fn equal_noteworthy(&self, other: &Metadata) -> bool {
        self.uid == other.uid
            && self.gid == other.gid
            && self.len == other.len
            && self.mode == other.mode
            && self.modified == other.modified
    }
}

#[derive(Debug, Clone)]
pub struct Fingerprint {
    content_hash: u64,
    modified: DateTime<Utc>,
    tree_data: BTreeMap<String, Metadata>,
}

impl Fingerprint {
    pub fn content_hash(&self) -> u64 {
        self.content_hash
    }

    pub fn modified(&self) -> DateTime<Utc> {
        self.modified
    }

    #[allow(dead_code)]
    pub fn changes_from(&self, other: &Fingerprint) -> Vec<(String, DeltaAction)> {
        let mut result = Vec::new();
        let mut from_iter = other.tree_data.iter();
        let mut to_iter = self.tree_data.iter();

        let mut from = None;
        let mut to = None;
        loop {
            if from.is_none() {
                from = from_iter.next();
            }
            if to.is_none() {
                to = to_iter.next();
            }
            match (from, to) {
                (Some(from_entry), Some(to_entry)) => {
                    if from_entry.0 < to_entry.0 {
                        result.push((from_entry.0.clone(), DeltaAction::Removed));
                        from = None;
                    } else if from_entry.0 > to_entry.0 {
                        result.push((to_entry.0.clone(), DeltaAction::Added));
                        to = None;
                    } else {
                        if !from_entry.1.equal_noteworthy(&to_entry.1) {
                            result.push((from_entry.0.clone(), DeltaAction::Changed));
                        }
                        from = None;
                        to = None;
                    }
                }
                (Some(from_entry), None) => {
                    result.push((from_entry.0.clone(), DeltaAction::Removed));
                    from = None;
                }
                (None, Some(to_entry)) => {
                    result.push((to_entry.0.clone(), DeltaAction::Added));
                    to = None;
                }
                (None, None) => break,
            }
        }
        result
    }
}

pub async fn fingerprint_directory(path: &Path) -> Result<Fingerprint, JsValue> {
    let ignores = Ignores::default();
    fingerprint_directory_with_ignores(path, &ignores).await
}

pub async fn fingerprint_directory_with_ignores(path: &Path, ignores: &Ignores) -> Result<Fingerprint, JsValue> {
    fingerprint_directory_with_ignores_impl(0, path, ignores).await
}

#[async_recursion(?Send)]
pub async fn fingerprint_directory_with_ignores_impl(
    depth: usize,
    path: &Path,
    ignores: &Ignores,
) -> Result<Fingerprint, JsValue> {
    let mut tree_data = BTreeMap::new();
    let stats = fs::symlink_metadata(path).await?;
    let mut modified = stats.modified();
    let dir = fs::read_dir(path).await?;
    let mut map = BTreeMap::new();
    for entry in dir {
        if ignores.should_ignore(entry.file_name().as_str(), depth) {
            continue;
        }
        let path = entry.path();
        let file_type = entry.file_type();
        let (hash, child_modified) = if file_type.is_dir() {
            let fingerprint = fingerprint_directory_with_ignores_impl(depth + 1, &path, ignores).await?;
            tree_data.extend(fingerprint.tree_data);
            (fingerprint.content_hash, fingerprint.modified)
        } else {
            let stats = fs::symlink_metadata(&path).await?;
            let metadata = Metadata::from(&stats);
            let mut hasher = DefaultHasher::default();
            metadata.hash_noteworthy(&mut hasher);
            tree_data.insert(path.to_string(), metadata);
            (hasher.finish(), modified)
        };
        map.insert(path.to_string(), hash);
        modified = std::cmp::max(modified, child_modified);
    }
    let mut hasher = DefaultHasher::default();
    map.hash(&mut hasher);
    let result = Fingerprint {
        content_hash: hasher.finish(),
        modified,
        tree_data,
    };
    Ok(result)
}
