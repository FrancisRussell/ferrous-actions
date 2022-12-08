use crate::action_paths::get_action_cache_dir;
use crate::actions::exec::Command;
use crate::actions::io;
use crate::cargo_hook::CargoHook;
use crate::node::path::Path;
use crate::{actions, info, node, nonce, Cargo, Error};

async fn create_empty_dir() -> Result<Path, Error> {
    let nonce = nonce::build_nonce(8);
    let mut path = get_action_cache_dir()?;
    path.push("empty-directories");
    path.push(nonce.to_string().as_str());
    node::fs::create_dir_all(&path).await?;
    Ok(path)
}

struct ChangeCwdHook {
    new_cwd: String,
}

impl CargoHook for ChangeCwdHook {
    fn modify_command(&self, command: &mut Command) {
        let path = Path::from(self.new_cwd.as_str());
        command.current_dir(&path);
    }
}

#[derive(Clone, Debug)]
pub struct Cross {
    path: Path,
}

impl Cross {
    pub async fn get() -> Result<Cross, Error> {
        io::which("cross", true)
            .await
            .map(|path| Cross { path })
            .map_err(Error::Js)
    }

    pub async fn get_or_install() -> Result<Cross, Error> {
        match Self::get().await {
            Ok(cross) => Ok(cross),
            Err(e) => {
                info!("Unable to find cross: {:?}", e);
                info!("Installing it now...");
                Self::install().await
            }
        }
    }

    async fn install() -> Result<Cross, Error> {
        let mut cargo = Cargo::from_environment().await?;
        let args = ["cross"];
        // Due to the presence of rust toolchain files, actions-rs decides to change
        // directory before invoking cargo install. We do the same.
        let cwd = create_empty_dir().await?;
        let hook = ChangeCwdHook {
            new_cwd: cwd.to_string(),
        };
        cargo.run_with_hook(None, "install", args, hook).await?;
        // Ignore failure just in case there's some random process still hanging around
        drop(actions::io::rm_rf(&cwd).await);
        Self::get().await
    }

    pub fn get_path(&self) -> Path {
        self.path.clone()
    }
}
