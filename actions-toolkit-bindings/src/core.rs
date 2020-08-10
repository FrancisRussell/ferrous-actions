use js_sys::JsString;
use wasm_bindgen::prelude::*;

// These bindings are incomplete.
// Only bindings used by this example project have been defined.

#[wasm_bindgen]
pub struct InputOptions {
    pub required: Option<bool>,
}

#[wasm_bindgen(module = "@actions/core")]
extern {
    /// Gets the value of an input. The value is also trimmed.
    #[wasm_bindgen(js_name = "getInput")]
    pub fn get_input(name: &JsString, options: Option<InputOptions>) -> JsString;

    /// Writes info to log with console.log.
    #[wasm_bindgen]
    pub fn info(message: &JsString);

    /// Sets the action status to failed.
    /// When the action exits it will be with an exit code of 1.
    #[wasm_bindgen(js_name = "setFailed")]
    pub fn set_failed(message: &JsString);

    /// Sets the value of an output.
    #[wasm_bindgen(js_name = "setOutput")]
    pub fn set_output(name: &JsString, value: &JsString);
}
