use crate::actions::{core, exec, io, tool_cache};
use crate::info;
use crate::node;
use crate::node::path::Path;
use crate::Error;

#[derive(Clone, Debug)]
pub struct ToolchainConfig {
    pub name: String,
    pub profile: String,
    pub components: Vec<String>,
    pub targets: Vec<String>,
}

impl Default for ToolchainConfig {
    fn default() -> ToolchainConfig {
        ToolchainConfig {
            name: "stable".into(),
            profile: "default".into(),
            components: Vec::new(),
            targets: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Rustup {
    path: Path,
}

impl Rustup {
    pub async fn get_or_install() -> Result<Rustup, Error> {
        match Self::get().await {
            Ok(rustup) => Ok(rustup),
            Err(e) => {
                info!("Unable to find rustup: {:?}", e);
                info!("Installing it now");
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
        info!("Getting rustup for platform: {:?}", platform);
        match platform.as_str() {
            "darwin" | "linux" => {
                let rustup_script = tool_cache::download_tool("https://sh.rustup.rs")
                    .await
                    .map_err(Error::Js)?;
                info!("Downloaded to: {:?}", rustup_script);
                node::fs::chmod(&rustup_script, 0x755)
                    .await
                    .map_err(Error::Js)?;
                exec::exec(&rustup_script, args).await.map_err(Error::Js)?;
                let mut cargo_path = node::os::homedir();
                cargo_path.push(".cargo");
                cargo_path.push("bin");
                info!("Adding to path {:?}", cargo_path);
                core::add_path(&cargo_path);
            }
            "windows" => {
                let rustup_exe = tool_cache::download_tool("https://win.rustup.rs")
                    .await
                    .map_err(Error::Js)?;
                info!("Downloaded to: {:?}", rustup_exe);
                exec::exec(&rustup_exe, args).await.map_err(Error::Js)?;
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

    pub async fn install_toolchain(&self, config: &ToolchainConfig) -> Result<(), Error> {
        let mut args: Vec<_> = ["toolchain", "install"]
            .into_iter()
            .map(String::from)
            .collect();
        args.push(config.name.clone());
        args.extend(["--profile".into(), config.profile.clone()]);
        for target in &config.targets {
            args.extend(["-t".into(), target.clone()]);
        }
        // It seems that components can take multiple arguments so the toolchain name
        // must be present before this
        for component in &config.components {
            args.extend(["-c".into(), component.clone()]);
        }
        exec::exec(&self.path, args).await.map_err(Error::Js)?;
        Ok(())
    }

    pub fn get_path(&self) -> &Path {
        &self.path
    }
}
