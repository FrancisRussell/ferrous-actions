use actions_toolkit_bindings::{core, io};
use std::path::PathBuf;
use wasm_bindgen::JsValue;

#[derive(Debug)]
pub struct Rustup {
    path: PathBuf,
}

impl Rustup {
    pub async fn get_or_install() -> Result<Rustup, JsValue> {
        match Self::get().await {
            Ok(rustup) => Ok(rustup),
            Err(e) => {
                core::info(format!("Unable to find rustup: {:?}", e));
                core::info("Installing it now");
                Self::install().await
            }
        }
    }

    pub async fn get() -> Result<Rustup, JsValue> {
        io::which("rustup", true).await.map(|path| Rustup { path })
    }

    pub async fn install() -> Result<Rustup, JsValue> {
        let args = ["--default-toolchain", "none", "-y"];
        let platform = node_sys::os::platform();
        core::info(format!("Platform: {:?}", platform));
        todo!("Rustup::install")
    }
}
