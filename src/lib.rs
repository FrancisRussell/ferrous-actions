pub mod actions;
mod annotation_hook;
mod cache_cargo_home;
mod cargo;
mod cargo_hook;
mod cargo_install_hook;
mod date;
mod error;
mod fingerprinting;
pub mod node;
mod nonce;
mod noop_stream;
mod package_manifest;
mod run;
mod rustup;
mod toolchain;
mod utils;

use crate::actions::core;
use crate::cargo::Cargo;
use crate::error::Error;
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
