pub mod actions;
mod cargo;
mod error;
pub mod node;
mod noop_stream;
mod run;
mod rustup;
mod utils;

use crate::actions::core;
use crate::cargo::Cargo;
use crate::error::Error;
use crate::rustup::Rustup;
use wasm_bindgen::prelude::{wasm_bindgen, JsValue};

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    // Perhaps we need a hook that calls core::set_failed() on panic.
    // This would make sure the action outputs an error command for
    // the runner and returns exit code 1.
    utils::set_panic_hook();

    if let Err(e) = run::run().await {
        let msg = format!("{}", e);
        core::set_failed(msg);
    }
    Ok(())
}
