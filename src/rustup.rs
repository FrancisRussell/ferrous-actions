use crate::actions::exec::Command;
use crate::actions::{core, io, tool_cache};
use crate::node::path::Path;
use crate::{debug, info, node, Error};
use parking_lot::Mutex;
use std::sync::Arc;

const NO_DEFAULT_TOOLCHAIN_NAME: &str = "none";

pub async fn install_rustup(toolchain_config: &ToolchainConfig) -> Result<(), Error> {
    let rustup = Rustup::get_or_install().await?;
    debug!("Rustup installed at: {}", rustup.get_path());
    rustup.update().await?;
    rustup.install_toolchain(toolchain_config).await?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ToolchainConfig {
    pub name: String,
    pub profile: String,
    pub components: Vec<String>,
    pub targets: Vec<String>,
    pub default: bool,
}

impl Default for ToolchainConfig {
    fn default() -> ToolchainConfig {
        ToolchainConfig {
            name: "stable".into(),
            profile: "default".into(),
            components: Vec::new(),
            targets: Vec::new(),
            default: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Rustup {
    path: Path,
}

impl Rustup {
    pub async fn get_or_install() -> Result<Rustup, Error> {
        match Self::get().await {
            Ok(rustup) => Ok(rustup),
            Err(e) => {
                info!("Unable to find rustup, Installing it now...");
                debug!("Attempting to locate rustup returned this error: {}", e);
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
        let args = ["--default-toolchain", NO_DEFAULT_TOOLCHAIN_NAME, "-y"];
        let platform = node::os::platform();
        info!("Getting rustup for platform: {:?}", platform);
        match platform.as_str() {
            "darwin" | "linux" => {
                let rustup_script = tool_cache::download_tool("https://sh.rustup.rs")
                    .await
                    .map_err(Error::Js)?;
                info!("Downloaded to: {:?}", rustup_script);
                node::fs::chmod(&rustup_script, 0x755).await.map_err(Error::Js)?;
                Command::from(&rustup_script)
                    .args(args)
                    .exec()
                    .await
                    .map_err(Error::Js)?;
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
                Command::from(&rustup_exe).args(args).exec().await.map_err(Error::Js)?;
            }
            _ => return Err(Error::UnsupportedPlatform(platform)),
        }
        Self::get().await
    }

    pub async fn update(&self) -> Result<(), Error> {
        Command::from(&self.path)
            .arg("update")
            .exec()
            .await
            .map_err(Error::Js)?;
        Ok(())
    }

    pub async fn install_toolchain(&self, config: &ToolchainConfig) -> Result<(), Error> {
        if config.name == NO_DEFAULT_TOOLCHAIN_NAME {
            return Ok(());
        }
        let mut args: Vec<_> = ["toolchain", "install"].into_iter().map(String::from).collect();
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
        Command::from(&self.path).args(args).exec().await.map_err(Error::Js)?;
        if config.default {
            Command::from(&self.path)
                .arg("default")
                .arg(config.name.clone())
                .exec()
                .await
                .map_err(Error::Js)?;
        }
        Ok(())
    }

    pub async fn installed_toolchains(&self) -> Result<Vec<String>, Error> {
        let args: Vec<_> = ["toolchain", "list"].into_iter().map(String::from).collect();

        let toolchains: Arc<Mutex<Vec<String>>> = Default::default();
        {
            let match_default = regex::Regex::new(r" *\(default\) *$").expect("Regex compilation failed");
            let toolchains = Arc::clone(&toolchains);
            Command::from(&self.path)
                .args(args)
                .outline(move |line| {
                    let toolchain = match_default.replace(line, "");
                    toolchains.lock().push(toolchain.to_string());
                })
                .exec()
                .await
                .map_err(Error::Js)?;
        }
        let toolchains = toolchains.lock().drain(..).collect();
        Ok(toolchains)
    }

    pub async fn install_component(&self, name: &str) -> Result<(), Error> {
        Command::from(&self.path)
            .arg("component")
            .arg("add")
            .arg(name)
            .exec()
            .await
            .map_err(Error::Js)?;
        Ok(())
    }

    pub fn get_path(&self) -> &Path {
        &self.path
    }
}
