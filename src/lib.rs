#![warn(clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::uninlined_format_args,
    clippy::missing_panics_doc
)]
#![forbid(unsafe_code)]

//! This crate can be compiled so that only the node.js, and optionally also the
//! GitHub Actions Toolkit bindings, are present. To do this, use the
//! `default-features = false` option when depending on this crate and specify
//! the `node_bindings` and / or `github_actions_bindings` features.
//!
//! This crate is versioned according to compatibility of the "Ferrous actions"
//! action, not API compatibility. These bindings can and likely will change in
//! breaking ways and are mainly exposed and documented to enable more insight
//! into this crate's internals and for experimentation purposes.

pub mod log;

/// Bindings for [node.js](https://nodejs.org/docs/latest/api/)
#[cfg(feature = "node_bindings")]
pub mod node;

#[cfg(feature = "node_bindings")]
mod system;

/// Bindings for the [GitHub Actions Toolkit](https://github.com/actions/toolkit)
#[cfg(feature = "github_actions_bindings")]
pub mod actions;


cfg_if::cfg_if! {
if #[cfg(feature = "action")] {

mod access_times;
mod action_paths;
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
mod lossy_line_splitter;
mod nonce;
mod package_manifest;
mod push_line_splitter;
mod run;
mod rustup;
mod safe_encoding;
mod toolchain;
mod utils;

use crate::cargo::Cargo;
use crate::error::Error;

/// Entry point for Ferrous Actions.
///
/// If you want to use the node.js or GitHub Actions Toolkit bindings, you must set
/// `default-features = false` and enable only the bindings you want so this function
/// is not included in the dependency.
#[cfg(feature = "action")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub async fn start() -> Result<(), wasm_bindgen::JsValue> {
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

}
}
