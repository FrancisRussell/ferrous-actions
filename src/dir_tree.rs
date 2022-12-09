use crate::node::fs;
use crate::node::path::Path;
use crate::Error;
use async_recursion::async_recursion;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};

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
pub async fn apply_visitor_impl<V>(
    depth: usize,
    folder_path: &Path,
    ignores: &Ignores,
    visitor: &mut V,
) -> Result<(), Error>
where
    V: DirTreeVisitor,
{
    let dir = fs::read_dir(folder_path).await?;
    for entry in dir {
        let file_name = entry.file_name();
        if ignores.should_ignore(&file_name, depth) {
            continue;
        }
        let file_type = entry.file_type();
        let path = entry.path();
        if file_type.is_dir() {
            visitor.enter_folder(&path).await?;
            apply_visitor_impl(depth + 1, &path, ignores, visitor).await?;
            visitor.exit_folder(&path).await?;
        } else {
            visitor.visit_file(&path).await?;
        }
    }
    Ok(())
}
