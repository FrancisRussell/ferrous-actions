pub use crate::dir_tree::Ignores;
use crate::dir_tree::{apply_visitor, DirTreeVisitor};
use crate::node::fs;
use crate::node::path::{self, Path};
use crate::Error;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
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

#[derive(Debug)]
struct FlatteningIterator<'a> {
    separator: String,
    stack: VecDeque<(String, btree_map::Iter<'a, String, Entry>)>,
}

impl<'a> Iterator for FlatteningIterator<'a> {
    type Item = (Cow<'a, str>, &'a Metadata);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((path, iter)) = self.stack.back_mut() {
            let pop = {
                match iter.next() {
                    None => true,
                    Some((base_name, entry)) => {
                        let mut path = path.clone();
                        path.extend([self.separator.as_str(), base_name]);
                        match entry {
                            Entry::File(metadata) => return Some((path.into(), metadata)),
                            Entry::Dir(sub_tree) => {
                                self.stack.push_back((path, sub_tree.iter()));
                                false
                            }
                        }
                    }
                }
            };
            if pop {
                self.stack.pop_back();
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

    pub fn modified(&self) -> DateTime<Utc> {
        self.modified
    }

    fn sorted_file_paths_and_metadata(&self) -> FlatteningIterator<'_> {
        FlatteningIterator {
            stack: VecDeque::from([(".".into(), self.tree_data.iter())]),
            separator: path::separator(),
        }
    }

    pub fn changes_from(&self, other: &Fingerprint) -> Vec<DeltaItem> {
        use itertools::{EitherOrBoth, Itertools as _};

        let from_iter = other.sorted_file_paths_and_metadata();
        let to_iter = self.sorted_file_paths_and_metadata();
        from_iter
            .merge_join_by(to_iter, |left, right| left.0.cmp(&right.0))
            .filter_map(|element| match element {
                EitherOrBoth::Both(left, right) => {
                    if !left.1.equal_noteworthy(right.1) {
                        Some((left.0.into_owned(), DeltaAction::Changed))
                    } else {
                        None
                    }
                }
                EitherOrBoth::Left(left) => Some((left.0.into_owned(), DeltaAction::Removed)),
                EitherOrBoth::Right(right) => Some((right.0.into_owned(), DeltaAction::Added)),
            })
            .collect()
    }
}

#[allow(dead_code)]
pub async fn fingerprint_directory(path: &Path) -> Result<Fingerprint, Error> {
    let ignores = Ignores::default();
    fingerprint_directory_with_ignores(path, &ignores).await
}

struct FingerprintVisitor {
    tree_data_stack: VecDeque<BTreeMap<String, Entry>>,
    modified: Option<DateTime<Utc>>,
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
        self.modified = Some(match self.modified {
            None => metadata.modified,
            Some(latest) => std::cmp::max(latest, metadata.modified),
        });
        let entry = Entry::File(metadata);
        self.tree_data().insert(file_name, entry);
        Ok(())
    }
}

pub async fn fingerprint_directory_with_ignores(path: &Path, ignores: &Ignores) -> Result<Fingerprint, Error> {
    let mut visitor = FingerprintVisitor {
        tree_data_stack: VecDeque::from([BTreeMap::new()]),
        modified: None,
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
    let modified = if let Some(modified) = visitor.modified {
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
