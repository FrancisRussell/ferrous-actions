use crate::actions::{core, exec, io, tool_cache};
use crate::node;
use wasm_bindgen::JsValue;

#[derive(Debug)]
pub struct Rustup {
    path: String,
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
        let platform = String::from(node::os::platform());
        match platform.as_str() {
            "darwin" | "linux" => {
                let rustup_script = tool_cache::download_tool(
                    "https://sh.rustup.rs",
                    &tool_cache::DownloadParams::default(),
                )
                .await?;
                core::info(format!("Downloaded to: {:?}", rustup_script));
                node::fs::chmod(rustup_script.as_str(), 0x755).await?;
                exec::exec(rustup_script.as_str(), args).await?;
                todo!("Add rust to path");
            }
            _ => panic!("Unsupported platform: {}", platform),
        }
        core::info(format!("Platform: {:?}", platform));
    }
}
