use crate::node::path::Path;
use js_sys::JsString;
use wasm_bindgen::JsValue;

pub async fn which<T: Into<JsString>>(tool: T, check: bool) -> Result<Path, JsValue> {
    let path = ffi::which(&tool.into(), Some(check)).await?;
    let path: JsString = path.into();
    Ok(Path::from(path))
}

pub mod ffi {
    use js_sys::JsString;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "@actions/io")]
    extern "C" {
        #[wasm_bindgen(js_name = "which", catch)]
        pub async fn which(tool: &JsString, check: Option<bool>) -> Result<JsValue, JsValue>;
    }
}
