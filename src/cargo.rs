use crate::actions::exec::Command;
use crate::actions::io;
use crate::annotation_hook::AnnotationHook;
use crate::cargo_hook::CargoHook;
use crate::cargo_hook::CompositeCargoHook;
use crate::node::path::Path;
use crate::Error;
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

    async fn get_hooks_for_subcommand(subcommand: &str) -> Result<Box<dyn CargoHook>, Error> {
        let mut hooks = CompositeCargoHook::default();
        match subcommand {
            "build" | "check" | "clippy" => {
                hooks.push(AnnotationHook::new(subcommand).await?);
            }
            _ => {}
        }
        Ok(Box::new(hooks))
    }

    pub async fn run<'a, I>(
        &'a mut self,
        toolchain: Option<&str>,
        subcommand: &'a str,
        args: I,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let mut final_args = Vec::new();
        if let Some(toolchain) = toolchain {
            final_args.push(format!("+{}", toolchain));
        }
        let mut hooks = Self::get_hooks_for_subcommand(subcommand).await?;
        final_args.push(subcommand.into());
        final_args.extend(
            hooks
                .additional_cargo_options()
                .into_iter()
                .map(Cow::into_owned),
        );
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
