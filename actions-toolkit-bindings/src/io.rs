use js_sys::JsString;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "@actions/io")]
extern "C" {
    /// Gets the value of an input. The value is also trimmed.
    #[wasm_bindgen(js_name = "which", catch)]
    pub async fn which(tool: &JsString, check: Option<bool>) -> Result<JsValue, JsValue>;
}
