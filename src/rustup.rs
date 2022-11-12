use actions_toolkit_bindings::{core, io};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Rustup {
    path: PathBuf,
}

impl Rustup {
    pub async fn get_or_install() -> Rustup {
        todo!()
    }

    pub async fn get() -> Option<Rustup> {
        if let Ok(path) = io::which("rustup", true).await {
            Some(Rustup { path })
        } else {
            None
        }
    }
}
