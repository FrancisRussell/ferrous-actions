use crate::actions::io;
use crate::node::path::Path;
use crate::{debug, info, input_manager, Cargo, Error};

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

    pub async fn get_or_install(input_manager: &input_manager::Manager) -> Result<Cross, Error> {
        match Self::get().await {
            Ok(cross) => Ok(cross),
            Err(e) => {
                info!("Unable to find cross. Installing it now...");
                debug!("Attempting to locate cross returned this error: {}", e);
                Self::install(input_manager).await
            }
        }
    }

    async fn install(input_manager: &input_manager::Manager) -> Result<Cross, Error> {
        let mut cargo = Cargo::from_environment().await?;
        let args = ["cross"];
        cargo.run(None, "install", args, input_manager).await?;
        Self::get().await
    }

    pub fn get_path(&self) -> Path {
        self.path.clone()
    }
}
