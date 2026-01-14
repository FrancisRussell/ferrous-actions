use crate::action_paths::get_action_cache_dir;
use crate::actions::io;
use crate::cargo_hooks::{
    Annotation as AnnotationHook, Composite as CompositeHook, Hook as CargoHook, Install as CargoInstallHook,
};
use crate::input_manager::{self, Input};
use crate::lossy_line_splitter::LossyLineSplitter;
use crate::node::child_process::{Command, Stdio};
use crate::node::path::Path;
use crate::node::process;
use crate::{node, nonce, Error};
use std::borrow::Cow;

const CRATE_NAME_PATTERN: &str = r"(([[:word:]]|-)+)";
const SEMVER_PATTERN: &str = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?";

async fn create_empty_dir() -> Result<Path, Error> {
    let nonce = nonce::build(8);
    let path = get_action_cache_dir()?
        .join("empty-directories")
        .join(&nonce.to_string());
    node::fs::create_dir_all(&path).await?;
    Ok(path)
}

struct ChangeCwdHook {
    new_cwd: String,
}

impl CargoHook for ChangeCwdHook {
    fn modify_command(&self, command: &mut Command) {
        let path = Path::from(&self.new_cwd);
        command.current_dir(&path);
    }
}

#[derive(Clone, Debug)]
pub struct Cargo {
    path: Path,
}

#[derive(Clone, Debug)]
pub struct ToolchainVersion {
    long: String,
}

impl ToolchainVersion {
    pub fn short(&self) -> Cow<'_, str> {
        self.long.lines().next().unwrap_or_default().trim().into()
    }

    pub fn long(&self) -> Cow<'_, str> {
        self.long.as_str().into()
    }
}

impl Cargo {
    pub async fn from_environment() -> Result<Cargo, Error> {
        io::which("cargo", true)
            .await
            .map(|path| Cargo { path })
            .map_err(Error::Js)
    }

    pub async fn from_path(path: &Path) -> Result<Cargo, Error> {
        let full_path = process::cwd().join(path);
        if !full_path.exists().await {
            return Err(Error::PathDoesNotExist(full_path.to_string()));
        }
        let result = Cargo { path: full_path };
        Ok(result)
    }

    pub async fn get_installed(&self) -> Result<Vec<String>, Error> {
        use futures::{AsyncBufReadExt as _, StreamExt as _};

        // This was added to help remove non-Rustup installed cargo-fmt and rustfmt on
        // the GitHub runners. However the binaries do not appear to be
        // cargo-managed either.

        let cargo_line_pattern = format!("^{} v{}:", CRATE_NAME_PATTERN, SEMVER_PATTERN);
        let match_install = regex_lite::Regex::new(&cargo_line_pattern).expect("Regex compilation failed");
        let mut installs = Vec::new();
        let mut child = Command::from(&self.path)
            .args(["install", "--list"])
            .stdout(Stdio::piped())
            .spawn()?;
        let mut lines = futures::io::BufReader::new(child.take_stdout().expect("Child stdout was missing")).lines();
        while let Some(line) = lines.next().await {
            let line = line?;
            if let Some(captures) = match_install.captures(&line) {
                let name = captures.get(1).expect("Capture missing").as_str();
                installs.push(name.to_string());
            }
        }
        child.wait_success().await?;
        Ok(installs)
    }

    async fn get_hooks_for_subcommand(
        &self,
        toolchain: Option<&str>,
        subcommand: &str,
        args: &[String],
        input_manager: &input_manager::Manager,
    ) -> Result<CompositeHook<'_>, Error> {
        let mut hooks = CompositeHook::default();
        match subcommand {
            "build" | "check" | "clippy" => {
                let enabled = if let Some(enabled) = input_manager.get(Input::Annotations) {
                    enabled
                        .parse::<bool>()
                        .map_err(|_| Error::OptionParse("annotations".into(), enabled.to_string()))?
                } else {
                    true
                };
                if enabled {
                    hooks.push(AnnotationHook::new(subcommand));
                }
            }
            "install" => {
                // Due to the presence of rust toolchain files, actions-rs decides to change
                // directory before invoking cargo install cross. We do the same for all
                // installs, not just cross.
                let empty_dir = create_empty_dir().await?;
                let compiler_version = self.get_toolchain_version(toolchain, Some(&empty_dir)).await?;
                let empty_cwd_hook = ChangeCwdHook {
                    new_cwd: empty_dir.to_string(),
                };
                hooks.push(CargoInstallHook::new(&compiler_version, args).await?);
                hooks.push(empty_cwd_hook);
            }
            _ => {}
        }
        Ok(hooks)
    }

    async fn get_toolchain_version(
        &self,
        toolchain: Option<&str>,
        cwd: Option<&Path>,
    ) -> Result<ToolchainVersion, Error> {
        use futures::{AsyncBufReadExt as _, StreamExt as _};

        let rustc_path = io::which("rustc", true).await?;
        let mut output = String::new();
        let mut command = Command::from(&rustc_path);
        if let Some(toolchain) = toolchain {
            command.arg(format!("+{}", toolchain).as_str());
        }
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        command.arg("-Vv").stdout(Stdio::piped());
        let mut child = command.spawn()?;
        let mut lines = futures::io::BufReader::new(child.take_stdout().expect("stdout unexpectedly missing")).lines();
        while let Some(line) = lines.next().await {
            let line = line?;
            output += &line;
            output += "\n";
        }
        child.wait_success().await?;
        Ok(ToolchainVersion { long: output })
    }

    pub async fn run<'a, I>(
        &'a mut self,
        toolchain: Option<&str>,
        subcommand: &'a str,
        args: I,
        input_manager: &input_manager::Manager,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        use futures::StreamExt as _;

        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let mut final_args = Vec::with_capacity(args.len());
        if let Some(toolchain) = toolchain {
            final_args.push(format!("+{}", toolchain));
        }
        let mut hooks = self
            .get_hooks_for_subcommand(toolchain, subcommand, &args[..], input_manager)
            .await?;
        final_args.push(subcommand.into());
        final_args.extend(hooks.additional_cargo_options().into_iter().map(Cow::into_owned));
        final_args.extend(args);
        let mut command = Command::from(&self.path);
        command.args(final_args).stdout(Stdio::piped());
        hooks.modify_command(&mut command);

        let mut child = command.spawn()?;
        let child_stdout = child.take_stdout().expect("Child stdout was missing");

        // Output from builds is more unpredicatable so use the lossy line splitter.
        let mut stdout_lines = LossyLineSplitter::new(child_stdout);

        while let Some(line) = stdout_lines.next().await {
            let line = line?;
            if !hooks.outline(&line).await {
                crate::info!("{}", line);
            }
        }

        if let Err(e) = child.wait_success().await.map_err(Error::Js) {
            hooks.failed().await;
            Err(e)
        } else {
            hooks.succeeded().await;
            Ok(())
        }
    }
}
