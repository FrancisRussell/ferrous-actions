use crate::actions::core::{self, Input};
use crate::Error;
use crate::{debug, info};
use crate::{rustup::ToolchainConfig, Cargo, Rustup};

pub async fn run() -> Result<(), Error> {
    // Get the action input.
    let actor = core::get_input("actor")?.unwrap_or_else(|| String::from("world"));

    // Greet the workflow actor.
    info!("Hello, {}!", actor);

    let command: String = Input::from("command").get_required()?;
    let split: Vec<&str> = command.split_whitespace().collect();
    match split[..] {
        ["install-rustup"] => install_rustup().await?,
        ["cargo", cargo_subcommand] => {
            let mut cargo = Cargo::from_environment().await?;
            let cargo_args = Input::from("args").get()?.unwrap_or_default();
            let cargo_args = shlex::split(&cargo_args)
                .ok_or_else(|| Error::ArgumentsParseError(cargo_args.clone()))?;
            let toolchain = core::get_input("toolchain")?;
            cargo
                .run(
                    toolchain.as_deref(),
                    cargo_subcommand,
                    cargo_args.iter().map(String::as_str),
                )
                .await?;
        }
        _ => return Err(Error::UnknownCommand(command)),
    }

    // Set the action output.
    core::set_output("result", "success");

    Ok(())
}

async fn install_rustup() -> Result<(), Error> {
    let rustup = Rustup::get_or_install().await?;
    debug!("Rustup installed at: {}", rustup.get_path());
    rustup.update().await?;
    let mut toolchain_config = ToolchainConfig::default();
    if let Some(toolchain) = core::get_input("toolchain")? {
        toolchain_config.name = toolchain;
    }
    if let Some(profile) = core::get_input("profile")? {
        toolchain_config.profile = profile;
    }
    if let Some(components) = core::get_input("components")? {
        toolchain_config.components = components.split_whitespace().map(String::from).collect();
    }
    if let Some(targets) = core::get_input("targets")? {
        toolchain_config.targets = targets.split_whitespace().map(String::from).collect();
    }
    if let Some(set_default) = core::get_input("default")? {
        let set_default = set_default
            .parse::<bool>()
            .map_err(|_| Error::OptionParseError("default".into(), set_default.clone()))?;
        toolchain_config.default = set_default;
    }
    rustup.install_toolchain(&toolchain_config).await?;
    Ok(())
}
