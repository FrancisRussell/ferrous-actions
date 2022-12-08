use crate::actions::exec::Command;
use crate::actions::io;
use crate::annotation_hook::AnnotationHook;
use crate::cargo_hook::{CargoHook, CompositeCargoHook};
use crate::cargo_install_hook::CargoInstallHook;
use crate::node::path::Path;
use crate::Error;
use rust_toolchain_manifest::HashValue;
use std::borrow::Cow;

#[derive(Debug)]
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

    async fn get_hooks_for_subcommand(
        &self,
        toolchain: Option<&str>,
        subcommand: &str,
        args: &[String],
    ) -> Result<Box<dyn CargoHook>, Error> {
        let mut hooks = CompositeCargoHook::default();
        match subcommand {
            "build" | "check" | "clippy" => {
                hooks.push(AnnotationHook::new(subcommand).await?);
            }
            "install" => {
                let compiler_hash = self.get_toolchain_hash(toolchain).await?;
                hooks.push(CargoInstallHook::new(&compiler_hash, args).await?);
            }
            _ => {}
        }
        Ok(Box::new(hooks))
    }

    async fn get_toolchain_hash(&self, toolchain: Option<&str>) -> Result<HashValue, Error> {
        use crate::actions::exec::Stdio;
        use crate::info;
        use parking_lot::Mutex;
        use std::sync::Arc;

        let rustc_path = io::which("rustc", true).await.map_err(Error::Js)?;
        let mut command = Command::from(&rustc_path);
        let output: Arc<Mutex<String>> = Default::default();
        let output_captured = output.clone();
        if let Some(toolchain) = toolchain {
            command.arg(format!("+{}", toolchain).as_str());
        }
        command.arg("--version");
        command
            .outline(move |line| {
                *output_captured.lock() += line;
            })
            .stdout(Stdio::null());
        command.exec().await?;
        let output: String = output.lock().trim().to_string();
        info!("Compiler version: {}", output);
        Ok(HashValue::from_bytes(output.as_bytes()))
    }

    pub async fn run<'a, I>(&'a mut self, toolchain: Option<&str>, subcommand: &'a str, args: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let mut final_args = Vec::new();
        if let Some(toolchain) = toolchain {
            final_args.push(format!("+{}", toolchain));
        }
        let mut hooks = self.get_hooks_for_subcommand(toolchain, subcommand, &args[..]).await?;
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
