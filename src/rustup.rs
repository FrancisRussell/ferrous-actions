use crate::actions::{core, exec, io, tool_cache};
use crate::node;
use crate::node::path::Path;
use crate::Error;

#[derive(Debug)]
pub struct Rustup {
    path: Path,
}

impl Rustup {
    pub async fn get_or_install() -> Result<Rustup, Error> {
        match Self::get().await {
            Ok(rustup) => Ok(rustup),
            Err(e) => {
                core::info(format!("Unable to find rustup: {:?}", e));
                core::info("Installing it now");
                Self::install().await
            }
        }
    }

    pub async fn get() -> Result<Rustup, Error> {
        io::which("rustup", true)
            .await
            .map(|path| Rustup { path })
            .map_err(Error::Js)
    }

    pub async fn install() -> Result<Rustup, Error> {
        let args = ["--default-toolchain", "none", "-y"];
        let platform = String::from(node::os::platform());
        core::info(format!("Getting rustup for platform: {:?}", platform));
        match platform.as_str() {
            "darwin" | "linux" => {
                let rustup_script = tool_cache::download_tool(
                    "https://sh.rustup.rs",
                    &tool_cache::DownloadParams::default(),
                )
                .await
                .map_err(Error::Js)?;
                core::info(format!("Downloaded to: {:?}", rustup_script));
                node::fs::chmod(&rustup_script, 0x755)
                    .await
                    .map_err(Error::Js)?;
                exec::exec(&rustup_script, args).await.map_err(Error::Js)?;
                let mut cargo_path = node::os::homedir();
                cargo_path.push(".cargo");
                cargo_path.push("bin");
                core::info(format!("Adding to path {:?}", cargo_path));
                core::add_path(&cargo_path);
            }
            _ => panic!("Unsupported platform: {}", platform),
        }
        Self::get().await
    }

    pub async fn update(&self) -> Result<(), Error> {
        exec::exec(&self.path, ["update"])
            .await
            .map_err(Error::Js)?;
        Ok(())
    }
}
