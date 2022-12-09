pub use crate::dir_tree::Ignores;
use crate::dir_tree::{apply_visitor, DirTreeVisitor};
use crate::node::fs;
use crate::node::path::{self, Path};
use crate::Error;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::collections::{btree_map, BTreeMap, VecDeque};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    uid: u64,
    gid: u64,
    len: u64,
    mode: u64,
    modified: DateTime<Utc>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, strum::Display)]
pub enum DeltaAction {
    Added,
    Removed,
    Changed,
}

pub type DeltaItem = (String, DeltaAction);

pub fn render_delta_items(items: &[DeltaItem]) -> String {
    use std::fmt::Write as _;
    let mut result = String::new();
    for (path, item) in items {
        writeln!(&mut result, "{}: {}", item, path).expect("Unable to write to string");
    }
    result
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

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Entry {
    File(Metadata),
    Dir(BTreeMap<String, Entry>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    content_hash: u64,
    modified: DateTime<Utc>,
    tree_data: BTreeMap<String, Entry>,
}

#[derive(Clone, Debug)]
struct FlatteningIterator<'a> {
    separator: String,
    iterator_stack: VecDeque<btree_map::Iter<'a, String, Entry>>,
    path_stack: VecDeque<String>,
}

impl<'a> Iterator for FlatteningIterator<'a> {
    type Item = (Cow<'a, str>, &'a Metadata);

    fn next(&mut self) -> Option<Self::Item> {
        while !self.iterator_stack.is_empty() {
            let pop = {
                let iter = self
                    .iterator_stack
                    .back_mut()
                    .expect("Iterator stack unexpectedly empty");
                match iter.next() {
                    None => true,
                    Some((base_name, entry)) => {
                        let mut path = self.path_stack.back().expect("Path stack unexpectedly empty").clone();
                        path += &self.separator;
                        path += base_name;
                        match entry {
                            Entry::File(metadata) => return Some((path.into(), metadata)),
                            Entry::Dir(sub_tree) => {
                                self.iterator_stack.push_back(sub_tree.iter());
                                self.path_stack.push_back(path);
                                false
                            }
                        }
                    }
                }
            };
            if pop {
                self.iterator_stack.pop_back();
                self.path_stack.pop_back();
            }
        }
        None
    }
}

impl Fingerprint {
    pub fn content_hash(&self) -> u64 {
        self.content_hash
    }

    fn compute_tree_hash(tree_data: &BTreeMap<String, Entry>) -> u64 {
        let mut hasher = DefaultHasher::default();
        for (name, entry) in tree_data {
            name.hash(&mut hasher);
            match entry {
                Entry::File(metadata) => {
                    metadata.hash_noteworthy(&mut hasher);
                }
                Entry::Dir(sub_tree) => {
                    let hash = Self::compute_tree_hash(sub_tree);
                    hash.hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    }

    fn find_latest_modification(tree_data: &BTreeMap<String, Entry>) -> Option<DateTime<Utc>> {
        let mut result = None;
        for entry in tree_data.values() {
            let modification = match entry {
                Entry::File(metadata) => Some(metadata.modified),
                Entry::Dir(sub_tree) => Self::find_latest_modification(sub_tree),
            };
            result = match result {
                None => modification,
                Some(existing) => Some({
                    if let Some(modification) = modification {
                        std::cmp::max(modification, existing)
                    } else {
                        existing
                    }
                }),
            };
        }
        result
    }

    pub fn modified(&self) -> DateTime<Utc> {
        self.modified
    }

    fn iter_paths_and_metadata(&self) -> FlatteningIterator<'_> {
        FlatteningIterator {
            iterator_stack: VecDeque::from([self.tree_data.iter()]),
            path_stack: VecDeque::from([".".into()]),
            separator: path::separator(),
        }
    }

    pub fn changes_from(&self, other: &Fingerprint) -> Vec<DeltaItem> {
        let mut result = Vec::new();
        let mut from_iter = other.iter_paths_and_metadata();
        let mut to_iter = self.iter_paths_and_metadata();

        let mut from = None;
        let mut to = None;
        loop {
            if from.is_none() {
                from = from_iter.next();
            }
            if to.is_none() {
                to = to_iter.next();
            }
            match (&from, &to) {
                (Some(from_entry), Some(to_entry)) => match from_entry.0.cmp(&to_entry.0) {
                    Ordering::Less => {
                        result.push((from_entry.0.to_string(), DeltaAction::Removed));
                        from = None;
                    }
                    Ordering::Greater => {
                        result.push((to_entry.0.to_string(), DeltaAction::Added));
                        to = None;
                    }
                    Ordering::Equal => {
                        if !from_entry.1.equal_noteworthy(to_entry.1) {
                            result.push((from_entry.0.to_string(), DeltaAction::Changed));
                        }
                        from = None;
                        to = None;
                    }
                },
                (Some(from_entry), None) => {
                    result.push((from_entry.0.to_string(), DeltaAction::Removed));
                    from = None;
                }
                (None, Some(to_entry)) => {
                    result.push((to_entry.0.to_string(), DeltaAction::Added));
                    to = None;
                }
                (None, None) => break,
            }
        }
        result
    }
}

#[allow(dead_code)]
pub async fn fingerprint_directory(path: &Path) -> Result<Fingerprint, Error> {
    let ignores = Ignores::default();
    fingerprint_directory_with_ignores(path, &ignores).await
}

struct FingerprintVisitor {
    tree_data_stack: VecDeque<BTreeMap<String, Entry>>,
}

impl FingerprintVisitor {
    fn tree_data(&mut self) -> &mut BTreeMap<String, Entry> {
        self.tree_data_stack.back_mut().expect("Missing tree data on stack")
    }
}

#[async_trait(?Send)]
impl DirTreeVisitor for FingerprintVisitor {
    async fn enter_folder(&mut self, _path: &Path) -> Result<(), Error> {
        self.tree_data_stack.push_back(BTreeMap::new());
        Ok(())
    }

    async fn exit_folder(&mut self, path: &Path) -> Result<(), Error> {
        let dir_data = self.tree_data_stack.pop_back().expect("Missing tree data stack entry");
        if !dir_data.is_empty() {
            let dir_name = path.file_name();
            let entry = Entry::Dir(dir_data);
            self.tree_data().insert(dir_name, entry);
        }
        Ok(())
    }

    async fn visit_file(&mut self, path: &Path) -> Result<(), Error> {
        let file_name = path.file_name();
        let stats = fs::symlink_metadata(path).await?;
        let metadata = Metadata::from(&stats);
        let entry = Entry::File(metadata);
        self.tree_data().insert(file_name, entry);
        Ok(())
    }
}

pub async fn fingerprint_directory_with_ignores(path: &Path, ignores: &Ignores) -> Result<Fingerprint, Error> {
    let mut visitor = FingerprintVisitor {
        tree_data_stack: VecDeque::from([BTreeMap::new()]),
    };
    apply_visitor(path, ignores, &mut visitor).await?;
    assert_eq!(
        visitor.tree_data_stack.len(),
        1,
        "Tree data stack should only have single entry"
    );
    let tree_data = visitor
        .tree_data_stack
        .pop_back()
        .expect("Tree data stack was unexpectedly empty");
    let content_hash = Fingerprint::compute_tree_hash(&tree_data);
    let modified = Fingerprint::find_latest_modification(&tree_data);
    let modified = if let Some(modified) = modified {
        modified
    } else {
        // If we have no files, then just use the folder modified time
        let stats = fs::symlink_metadata(path).await?;
        stats.modified()
    };
    let result = Fingerprint {
        content_hash,
        modified,
        tree_data,
    };
    Ok(result)
}
