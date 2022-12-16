pub use crate::dir_tree::Ignores;
use crate::dir_tree::{apply_visitor, DirTreeVisitor};
use crate::node::fs;
use crate::node::path::{self, Path};
use crate::Error;
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
    root: Entry,
}

#[derive(Debug)]
struct FlatteningIterator<'a> {
    separator: String,
    stack: VecDeque<(Option<String>, Either<btree_map::Iter<'a, String, Entry>, Metadata>)>,
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

    fn sorted_file_paths_and_metadata(&self) -> FlatteningIterator<'_> {
        let root_content = match &self.root {
            Entry::File(metadata) => Either::Right(*metadata),
            Entry::Dir(sub_tree) => Either::Left(sub_tree.iter()),
        };
        FlatteningIterator {
            stack: VecDeque::from([(None, root_content)]),
            separator: path::separator(),
        }
    }

    pub fn changes_from(&self, other: &Fingerprint) -> Vec<DeltaItem> {
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

    fn get_unaccessed_since_process_either<S>(
        path: &Path,
        element: EitherOrBoth<(S, &Entry), (S, &Entry)>,
        depth: usize,
    ) -> Vec<Path>
    where
        S: AsRef<str>,
    {
        let predicated = |element, condition| if condition { vec![element] } else { vec![] };
        match element {
            EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => vec![],
            EitherOrBoth::Both(left, right) => {
                let path = path.join(left.0.as_ref());
                match (left.1, right.1) {
                    (Entry::File(meta1), Entry::File(meta2)) => predicated(path, meta1 == meta2),
                    (Entry::Dir(tree1), Entry::Dir(tree2)) => {
                        if depth == 0 {
                            predicated(path, tree1 == tree2)
                        } else {
                            Self::get_unaccessed_process_iterators(&path, tree1.iter(), tree2.iter(), depth - 1)
                        }
                    }
                    (Entry::Dir(_), Entry::File(_)) | (Entry::File(_), Entry::Dir(_)) => vec![],
                }
            }
        }
    }

    fn get_unaccessed_process_iterators<'a, I, S>(path: &Path, from_iter: I, to_iter: I, depth: usize) -> Vec<Path>
    where
        I: Iterator<Item = (S, &'a Entry)>,
        S: AsRef<str>,
    {
        use itertools::Itertools as _;

        from_iter
            .merge_join_by(to_iter, |left, right| left.0.as_ref().cmp(right.0.as_ref()))
            .flat_map(|element| Self::get_unaccessed_since_process_either(path, element, depth))
            .collect()
    }

    pub fn get_unaccessed_since(&self, other: &Fingerprint, depth: usize) -> Vec<Path> {
        let root_path = Path::from(ROOT_NAME);
        let left = (ROOT_NAME, &other.root);
        let right = (ROOT_NAME, &self.root);
        let element = EitherOrBoth::Both(left, right);
        Self::get_unaccessed_since_process_either(&root_path, element, depth)
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
impl DirTreeVisitor for BuildFingerprintVisitor {
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

    async fn visit_file(&mut self, path: &Path) -> Result<(), Error> {
        let stats = fs::symlink_metadata(path).await?;
        let metadata = Metadata::from(&stats);
        self.modified = Some(match self.modified {
            None => metadata.modified,
            Some(latest) => std::cmp::max(latest, metadata.modified),
        });
        let file_name = path.file_name();
        self.push_file(file_name, metadata);
        Ok(())
    }
}

pub async fn fingerprint_path_with_ignores(path: &Path, ignores: &Ignores) -> Result<Fingerprint, Error> {
    let mut visitor = BuildFingerprintVisitor {
        stack: VecDeque::new(),
        modified: None,
    };
    apply_visitor(path, ignores, &mut visitor).await?;
    assert_eq!(visitor.stack.len(), 1, "Tree data stack should only have single entry");
    let root = visitor
        .stack
        .pop_back()
        .expect("Tree data stack was unexpectedly empty");
    let content_hash = Fingerprint::compute_entry_hash(&root);
    let result = Fingerprint {
        content_hash,
        modified: visitor.modified,
        root,
    };
    Ok(result)
}
