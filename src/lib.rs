#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::uninlined_format_args,
    clippy::missing_panics_doc
)]

mod access_times;
mod action_paths;
mod actions;
mod agnostic_path;
mod cache_cargo_home;
mod cache_key_builder;
mod cargo;
mod cargo_hooks;
mod cargo_lock_hashing;
mod cross;
mod delta;
mod dir_tree;
mod error;
mod fingerprinting;
mod hasher;
mod input_manager;
mod job;
mod node;
mod nonce;
mod noop_stream;
mod package_manifest;
mod run;
mod rustup;
mod safe_encoding;
mod serde_helpers;
mod system;
mod toolchain;
mod utils;

use crate::cargo::Cargo;
use crate::error::Error;
use wasm_bindgen::prelude::{wasm_bindgen, JsValue};

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    use crate::actions::core;

    // Perhaps we need a hook that calls core::set_failed() on panic.
    // This would make sure the action outputs an error command for
    // the runner and returns exit code 1.
    utils::set_panic_hook();

    if let Err(e) = run::run().await {
        core::set_failed(e.to_string());
    }
    Ok(())
}
