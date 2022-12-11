use crate::node::fs;
use crate::node::path::Path;
use crate::Error;
use async_recursion::async_recursion;
use async_trait::async_trait;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

pub const ROOT_NAME: &str = ".";

#[derive(Debug, Default, Clone)]
pub struct Ignores {
    map: HashMap<usize, HashSet<String>>,
}

impl Ignores {
    pub fn add(&mut self, depth: usize, name: &str) {
        self.map.entry(depth).or_default().insert(name.to_string());
    }

    pub fn should_ignore(&self, name: &str, depth: usize) -> bool {
        self.map.get(&depth).map(|names| names.contains(name)).unwrap_or(false)
    }
}

#[async_trait(?Send)]
pub trait DirTreeVisitor {
    async fn enter_folder(&mut self, path: &Path) -> Result<(), Error>;
    async fn exit_folder(&mut self, path: &Path) -> Result<(), Error>;
    async fn visit_file(&mut self, name: &Path) -> Result<(), Error>;
}

pub async fn apply_visitor<V>(folder_path: &Path, ignores: &Ignores, visitor: &mut V) -> Result<(), Error>
where
    V: DirTreeVisitor,
{
    apply_visitor_impl(0, folder_path, ignores, visitor).await
}

#[async_recursion(?Send)]
async fn apply_visitor_impl<V>(depth: usize, path: &Path, ignores: &Ignores, visitor: &mut V) -> Result<(), Error>
where
    V: DirTreeVisitor,
{
    let file_name: Cow<str> = if depth == 0 {
        ROOT_NAME.into()
    } else {
        path.file_name().into()
    };
    if ignores.should_ignore(&file_name, depth) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path).await?;
    if metadata.is_directory() {
        visitor.enter_folder(path).await?;
        let depth = depth + 1;
        let dir = fs::read_dir(path).await?;
        for entry in dir {
            let path = entry.path();
            apply_visitor_impl(depth, &path, ignores, visitor).await?;
        }
        visitor.exit_folder(path).await?;
    } else {
        visitor.visit_file(path).await?;
    }
    Ok(())
}
