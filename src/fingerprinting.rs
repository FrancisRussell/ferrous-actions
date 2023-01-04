use crate::delta::Action as DeltaAction;
pub use crate::dir_tree::Ignores;
use crate::node::fs;
use crate::node::path::{self, Path};
use crate::{dir_tree, Error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use itertools::{Either, EitherOrBoth};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::collections::{btree_map, BTreeMap, VecDeque};
use std::hash::{Hash, Hasher};

const ROOT_NAME: &str = ".";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
struct Metadata {
    uid: u64,
    gid: u64,
    len: u64,
    mode: u64,
    modified: DateTime<Utc>,
    accessed: DateTime<Utc>,
}

impl From<&fs::Metadata> for Metadata {
    fn from(stats: &fs::Metadata) -> Metadata {
        Metadata {
            uid: stats.uid(),
            gid: stats.gid(),
            len: stats.len(),
            mode: stats.mode(),
            modified: stats.modified(),
            accessed: stats.accessed(),
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

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
enum Entry {
    File(Metadata),
    Dir(BTreeMap<String, Entry>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    content_hash: u64,
    modified: Option<DateTime<Utc>>,
    accessed: Option<DateTime<Utc>>,
    root: Entry,
}

type BranchIter<'a> = btree_map::Iter<'a, String, Entry>;

#[derive(Debug)]
struct FlatteningIterator<'a> {
    separator: String,
    stack: VecDeque<(Option<String>, Either<BranchIter<'a>, Metadata>)>,
}

impl<'a> Iterator for FlatteningIterator<'a> {
    type Item = (Cow<'a, str>, Metadata);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((path, content)) = self.stack.pop_back() {
            match content {
                Either::Left(mut iter) => match iter.next() {
                    None => {}
                    Some((base_name, entry)) => {
                        let mut item_path = path.as_deref().unwrap_or("").to_string();
                        item_path += base_name;
                        self.stack.push_back((path, Either::Left(iter)));
                        match entry {
                            Entry::File(metadata) => return Some((item_path.into(), *metadata)),
                            Entry::Dir(sub_tree) => {
                                item_path += self.separator.as_str();
                                self.stack.push_back((Some(item_path), Either::Left(sub_tree.iter())));
                            }
                        }
                    }
                },
                Either::Right(metadata) => {
                    let path = path.unwrap_or_else(|| ROOT_NAME.to_string());
                    return Some((path.into(), metadata));
                }
            }
        }
        None
    }
}

impl Fingerprint {
    pub fn content_hash(&self) -> u64 {
        self.content_hash
    }

    fn compute_entry_hash(entry: &Entry) -> u64 {
        let mut hasher = DefaultHasher::default();
        match entry {
            Entry::File(metadata) => {
                metadata.hash_noteworthy(&mut hasher);
            }
            Entry::Dir(sub_tree) => {
                for (name, entry) in sub_tree {
                    name.hash(&mut hasher);
                    let hash = Self::compute_entry_hash(entry);
                    hash.hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    }

    pub fn modified(&self) -> Option<DateTime<Utc>> {
        self.modified
    }

    pub fn accessed(&self) -> Option<DateTime<Utc>> {
        self.accessed
    }

    fn sorted_file_paths_and_metadata(&self) -> FlatteningIterator<'_> {
        let root_content = match &self.root {
            Entry::File(metadata) => Either::Right(*metadata),
            Entry::Dir(sub_tree) => Either::Left(sub_tree.iter()),
        };
        FlatteningIterator {
            stack: VecDeque::from([(None, root_content)]),
            separator: path::separator().into(),
        }
    }

    pub fn changes_from(&self, other: &Fingerprint) -> Vec<(String, DeltaAction)> {
        use itertools::Itertools as _;

        let from_iter = other.sorted_file_paths_and_metadata();
        let to_iter = self.sorted_file_paths_and_metadata();
        from_iter
            .merge_join_by(to_iter, |left, right| left.0.cmp(&right.0))
            .filter_map(|element| match element {
                EitherOrBoth::Both(left, right) => {
                    (!left.1.equal_noteworthy(&right.1)).then(|| (left.0.into_owned(), DeltaAction::Changed))
                }
                EitherOrBoth::Left(left) => Some((left.0.into_owned(), DeltaAction::Removed)),
                EitherOrBoth::Right(right) => Some((right.0.into_owned(), DeltaAction::Added)),
            })
            .collect()
    }
}

#[allow(dead_code)]
pub async fn fingerprint_path(path: &Path) -> Result<Fingerprint, Error> {
    let ignores = Ignores::default();
    fingerprint_path_with_ignores(path, &ignores).await
}

struct BuildFingerprintVisitor {
    stack: VecDeque<Entry>,
    modified: Option<DateTime<Utc>>,
    accessed: Option<DateTime<Utc>>,
}

impl BuildFingerprintVisitor {
    fn push_file(&mut self, file_name: String, metadata: Metadata) {
        let to_insert = Entry::File(metadata);
        match self.stack.back_mut() {
            None => self.stack.push_back(to_insert),
            Some(entry) => match entry {
                Entry::File(_) => {
                    self.stack.push_back(to_insert);
                }
                Entry::Dir(ref mut map) => {
                    map.insert(file_name, to_insert);
                }
            },
        }
    }
}

#[async_trait(?Send)]
impl dir_tree::Visitor for BuildFingerprintVisitor {
    async fn enter_folder(&mut self, _path: &Path) -> Result<(), Error> {
        self.stack.push_back(Entry::Dir(BTreeMap::new()));
        Ok(())
    }

    async fn exit_folder(&mut self, path: &Path) -> Result<(), Error> {
        if self.stack.len() > 1 {
            let entry = self.stack.pop_back().expect("Missing tree visitor stack entry");
            let name = path.file_name();
            match self.stack.back_mut() {
                None => panic!("Missing parent entry on tree visitor stack"),
                Some(Entry::File(_)) => panic!("Parent entry on tree visitor stack wasn't a folder"),
                Some(Entry::Dir(map)) => {
                    map.insert(name, entry);
                }
            }
        }
        Ok(())
    }

    async fn visit_entry(&mut self, path: &Path, is_file: bool) -> Result<(), Error> {
        if is_file {
            let stats = fs::symlink_metadata(path).await?;
            let metadata = Metadata::from(&stats);
            self.modified = Some(match self.modified {
                None => metadata.modified,
                Some(latest) => std::cmp::max(latest, metadata.modified),
            });
            self.accessed = Some(match self.accessed {
                None => metadata.accessed,
                Some(latest) => std::cmp::max(latest, metadata.accessed),
            });
            let file_name = path.file_name();
            self.push_file(file_name, metadata);
        } else {
            panic!("Expected to descend into all directories");
        }
        Ok(())
    }
}

pub async fn fingerprint_path_with_ignores(path: &Path, ignores: &Ignores) -> Result<Fingerprint, Error> {
    let mut visitor = BuildFingerprintVisitor {
        stack: VecDeque::new(),
        modified: None,
        accessed: None,
    };
    dir_tree::apply_visitor(path, ignores, &mut visitor).await?;
    assert_eq!(visitor.stack.len(), 1, "Tree data stack should only have single entry");
    let root = visitor
        .stack
        .pop_back()
        .expect("Tree data stack was unexpectedly empty");
    let content_hash = Fingerprint::compute_entry_hash(&root);
    let result = Fingerprint {
        content_hash,
        modified: visitor.modified,
        accessed: visitor.accessed,
        root,
    };
    Ok(result)
}
