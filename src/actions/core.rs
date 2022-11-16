use crate::node::path::Path;
use js_sys::JsString;

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        $crate::actions::core::info(std::format!($($arg)*).as_str());
    }};
}

pub fn info<S: Into<JsString>>(message: S) {
    ffi::info(&message.into());
}

pub fn set_output<N: Into<JsString>, V: Into<JsString>>(name: N, value: V) {
    ffi::set_output(&name.into(), &value.into())
}

pub fn get_input<N: Into<JsString>>(name: N, options: Option<ffi::InputOptions>) -> Option<String> {
    let value: String = ffi::get_input(&name.into(), options).into();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub fn set_failed<M: Into<JsString>>(message: M) {
    ffi::set_failed(&message.into())
}

pub fn add_path(path: &Path) {
    ffi::add_path(&path.into())
}

#[allow(clippy::drop_non_drop)]
pub mod ffi {
    use js_sys::JsString;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct InputOptions {
        pub required: Option<bool>,

        #[wasm_bindgen(js_name = "trimWhitespace")]
        pub trim_whitespace: Option<bool>,
    }

    #[wasm_bindgen(module = "@actions/core")]
    extern "C" {
        /// Gets the value of an input. The value is also trimmed.
        #[wasm_bindgen(js_name = "getInput")]
        pub fn get_input(name: &JsString, options: Option<InputOptions>) -> JsString;

        /// Writes info to log with console.log.
        #[wasm_bindgen]
        pub fn info(message: &JsString);

        /// Writes debug to log with console.log.
        #[wasm_bindgen]
        pub fn debug(message: &JsString);

        /// Sets the action status to failed.
        /// When the action exits it will be with an exit code of 1.
        #[wasm_bindgen(js_name = "setFailed")]
        pub fn set_failed(message: &JsString);

        /// Sets the value of an output.
        #[wasm_bindgen(js_name = "setOutput")]
        pub fn set_output(name: &JsString, value: &JsString);

        #[wasm_bindgen(js_name = "addPath")]
        pub fn add_path(path: &JsString);

    }
}
