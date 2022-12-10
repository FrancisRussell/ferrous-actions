#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::uninlined_format_args,
    clippy::missing_panics_doc
)]

mod action_paths;
mod actions;
mod annotation_hook;
mod cache_cargo_home;
mod cargo;
mod cargo_hook;
mod cargo_install_hook;
mod cargo_lock_hashing;
mod cross;
mod date;
mod dir_tree;
mod error;
mod fingerprinting;
mod node;
mod nonce;
mod noop_stream;
mod package_manifest;
mod rng;
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
        core::set_failed(e.to_string());
    }
    Ok(())
}
