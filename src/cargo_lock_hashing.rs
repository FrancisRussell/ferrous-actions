use crate::dir_tree::{self, DirTreeVisitor, Ignores};
use crate::node::path::Path;
use crate::{node, Error};
use async_trait::async_trait;

#[derive(Debug)]
struct FindFilesVisitor {
    name: String,
    paths: Vec<Path>,
}

#[async_trait(?Send)]
impl DirTreeVisitor for FindFilesVisitor {
    async fn enter_folder(&mut self, _: &Path) -> Result<(), Error> {
        Ok(())
    }

    async fn exit_folder(&mut self, _: &Path) -> Result<(), Error> {
        Ok(())
    }

    async fn visit_entry(&mut self, path: &Path, is_file: bool) -> Result<(), Error> {
        if is_file && path.file_name() == self.name {
            self.paths.push(path.clone());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct HashInfo {
    pub num_files: usize,
    pub bytes: [u8; 32],
}

pub async fn hash_cargo_lock_files(path: &Path) -> Result<HashInfo, Error> {
    let mut visitor = FindFilesVisitor {
        name: "Cargo.lock".into(),
        paths: Vec::new(),
    };
    let ignores = Ignores::default();
    dir_tree::apply_visitor(path, &ignores, &mut visitor).await?;
    let mut paths: Vec<_> = visitor.paths.iter().map(Path::to_string).collect();
    // We want the paths in a deterministic order
    paths.sort();
    let mut hasher = blake3::Hasher::new();
    for path in &paths {
        let file_content = node::fs::read_file(path.as_str()).await?;
        hasher.update(&file_content);
    }
    let result = HashInfo {
        num_files: paths.len(),
        bytes: hasher.finalize().into(),
    };
    Ok(result)
}
