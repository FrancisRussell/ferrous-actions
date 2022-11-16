use crate::actions::core;
use crate::info;
use crate::Error;
use crate::{rustup::ToolchainConfig, Rustup};

pub async fn run() -> Result<(), Error> {
    // Get the action input.
    let actor = core::get_input("actor", None).unwrap_or_else(|| String::from("world"));

    // Greet the workflow actor.
    info!("Hello, {}!", actor);

    let command = core::get_input("command", None);
    if let Some(command) = command {
        match command.as_str() {
            "install-rustup" => install_rustup().await?,
            _ => panic!("Unknown command: {}", command),
        }
    } else {
        panic!("Command not specified!");
    }
    // Set the action output.
    // core::set_output("result", "success");

    Ok(())
}

async fn install_rustup() -> Result<(), Error> {
    let rustup = Rustup::get_or_install().await?;
    info!("Rustup installed at: {:?}", rustup);
    rustup.update().await?;
    let mut toolchain_config = ToolchainConfig::default();
    if let Some(toolchain) = core::get_input("toolchain", None) {
        toolchain_config.name = toolchain;
    }
    if let Some(profile) = core::get_input("profile", None) {
        toolchain_config.profile = profile;
    }
    if let Some(components) = core::get_input("components", None) {
        toolchain_config.components = components.split_whitespace().map(String::from).collect();
    }
    if let Some(targets) = core::get_input("targets", None) {
        toolchain_config.targets = targets.split_whitespace().map(String::from).collect();
    }
    rustup.install_toolchain(&toolchain_config).await?;
    Ok(())
}
