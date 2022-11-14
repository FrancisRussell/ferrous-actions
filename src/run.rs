use crate::actions::core;
use crate::Error;
use crate::Rustup;

pub async fn run() -> Result<(), Error> {
    // Get the action input.
    let actor = core::get_input("actor", None);

    // Greet the workflow actor.
    let greeting = format!("Hello, {}!", actor);
    core::info(greeting);

    let command = core::get_input("command", None);
    match command.as_str() {
        "install-rustup" => install_rustup().await?,
        _ => panic!("Unknown command: {}", command),
    }
    // Set the action output.
    // core::set_output("result", "success");

    Ok(())
}

async fn install_rustup() -> Result<(), Error> {
    let rustup = Rustup::get_or_install().await?;
    core::info(format!("Rustup installed at: {:?}", rustup));
    rustup.update().await?;
    Ok(())
}
