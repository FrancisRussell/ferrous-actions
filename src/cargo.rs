use crate::action_paths::get_action_cache_dir;
use crate::actions::exec::Command;
use crate::actions::io;
use crate::cargo_hooks::{
    Annotation as AnnotationHook, Composite as CompositeHook, Hook as CargoHook, Install as CargoInstallHook,
    Null as NullHook,
};
use crate::input_manager::{self, Input};
use crate::node::path::Path;
use crate::node::process;
use crate::{node, nonce, Error};
use rust_toolchain_manifest::HashValue;
use std::borrow::Cow;

async fn create_empty_dir() -> Result<Path, Error> {
    let nonce = nonce::build(8);
    let path = get_action_cache_dir()?
        .join("empty-directories")
        .join(nonce.to_string().as_str());
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
pub struct Cargo {
    path: Path,
}

impl Cargo {
    pub async fn from_environment() -> Result<Cargo, Error> {
        io::which("cargo", true)
            .await
            .map(|path| Cargo { path })
            .map_err(Error::Js)
    }

    pub async fn from_path(path: &Path) -> Result<Cargo, Error> {
        let full_path = process::cwd().join(path.clone());
        if !full_path.exists().await {
            return Err(Error::PathDoesNotExist(full_path.to_string()));
        }
        let result = Cargo { path: full_path };
        Ok(result)
    }

    pub async fn get_installed(&self) -> Result<Vec<String>, Error> {
        use parking_lot::Mutex;
        use std::sync::Arc;

        // This was added to help remove non-Rustup installed cargo-fmt and rustfmt on
        // the GitHub runners. However the binaries do not appear to be
        // cargo-managed either.

        let match_install =
            regex::Regex::new(r"^(([[:word:]]|-)+) v([[:digit:]]|\.)+:").expect("Regex compilation failed");
        let installs: Arc<Mutex<Vec<String>>> = Arc::default();
        let installs_captured = installs.clone();
        Command::from(&self.path)
            .args(["install", "--list"])
            .outline(move |line| {
                if let Some(captures) = match_install.captures(line) {
                    let name = captures.get(1).expect("Capture missing").as_str();
                    installs_captured.lock().push(name.to_string());
                }
            })
            .exec()
            .await
            .map_err(Error::Js)?;
        let installs = installs.lock().drain(..).collect();
        Ok(installs)
    }

    async fn get_hooks_for_subcommand(
        &self,
        toolchain: Option<&str>,
        subcommand: &str,
        args: &[String],
        input_manager: &input_manager::Manager,
    ) -> Result<CompositeHook, Error> {
        let mut hooks = CompositeHook::default();
        match subcommand {
            "build" | "check" | "clippy" => {
                let enabled = if let Some(enabled) = input_manager.get(Input::Annotations) {
                    enabled
                        .parse::<bool>()
                        .map_err(|_| Error::OptionParseError("annotations".into(), enabled.to_string()))?
                } else {
                    true
                };
                if enabled {
                    hooks.push(AnnotationHook::new(subcommand)?);
                }
            }
            "install" => {
                // Due to the presence of rust toolchain files, actions-rs decides to change
                // directory before invoking cargo install cross. We do the same for all
                // installs, not just cross.
                let empty_dir = create_empty_dir().await?;
                let compiler_hash = self.get_toolchain_hash(toolchain, Some(&empty_dir)).await?;
                let empty_cwd_hook = ChangeCwdHook {
                    new_cwd: empty_dir.to_string(),
                };
                hooks.push(CargoInstallHook::new(&compiler_hash, args).await?);
                hooks.push(empty_cwd_hook);
            }
            _ => {}
        }
        Ok(hooks)
    }

    async fn get_toolchain_hash(&self, toolchain: Option<&str>, cwd: Option<&Path>) -> Result<HashValue, Error> {
        use crate::actions::exec::Stdio;
        use parking_lot::Mutex;
        use std::sync::Arc;

        let rustc_path = io::which("rustc", true).await.map_err(Error::Js)?;
        let mut command = Command::from(&rustc_path);
        let output: Arc<Mutex<String>> = Arc::default();
        let output_captured = output.clone();
        if let Some(toolchain) = toolchain {
            command.arg(format!("+{}", toolchain).as_str());
        }
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        command.arg("-Vv");
        command
            .outline(move |line| {
                let mut out = output_captured.lock();
                *out += line;
                *out += "\n";
            })
            .stdout(Stdio::null());
        command.exec().await?;
        let output: String = output.lock().trim().to_string();
        Ok(HashValue::from_bytes(output.as_bytes()))
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
        self.run_with_hook_impl(toolchain, subcommand, args, NullHook::default(), input_manager)
            .await
    }

    pub async fn run_with_hook<'a, I, H>(
        &'a mut self,
        toolchain: Option<&str>,
        subcommand: &'a str,
        args: I,
        hook: H,
        input_manager: &input_manager::Manager,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a str>,
        H: CargoHook + Sync + 'a,
    {
        let mut opaque_hook = CompositeHook::default();
        opaque_hook.push(hook);
        self.run_with_hook_impl(toolchain, subcommand, args, opaque_hook, input_manager)
            .await
    }

    async fn run_with_hook_impl<'a, I, H>(
        &'a mut self,
        toolchain: Option<&str>,
        subcommand: &'a str,
        args: I,
        hook: H,
        input_manager: &input_manager::Manager,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a str>,
        H: CargoHook + Sync + 'a,
    {
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let mut final_args = Vec::new();
        if let Some(toolchain) = toolchain {
            final_args.push(format!("+{}", toolchain));
        }
        let mut hooks = self
            .get_hooks_for_subcommand(toolchain, subcommand, &args[..], input_manager)
            .await?;
        hooks.push(hook);
        final_args.push(subcommand.into());
        final_args.extend(hooks.additional_cargo_options().into_iter().map(Cow::into_owned));
        final_args.extend(args);
        let mut command = Command::from(&self.path);
        command.args(final_args);
        hooks.modify_command(&mut command);
        if let Err(e) = command.exec().await.map_err(Error::Js) {
            hooks.failed().await;
            Err(e)
        } else {
            hooks.succeeded().await;
            Ok(())
        }
    }
}
