use super::path;
use js_sys::JsString;
use std::borrow::Cow;
use std::sync::LazyLock;

static EOL: LazyLock<String> = LazyLock::new(|| {
    use wasm_bindgen::JsCast as _;
    ffi::EOL
        .clone()
        .dyn_into::<JsString>()
        .expect("eol wasn't a string")
        .into()
});

/// Returns the end-of-line marker for the platform
pub fn eol() -> Cow<'static, str> {
    EOL.as_str().into()
}

/// The name of the underlying platform
pub fn platform() -> String {
    ffi::platform().into()
}

/// The name of the machine type
pub fn machine() -> String {
    ffi::machine().into()
}

/// The architecture
pub fn arch() -> String {
    ffi::arch().into()
}

/// Path to the current user's home directory
pub fn homedir() -> path::Path {
    path::Path::from(ffi::homedir())
}

/// Path to the temporary directory
pub fn temp_dir() -> path::Path {
    path::Path::from(ffi::tmpdir())
}

/// Low-level bindings for node.js operating system functions
pub mod ffi {
    use js_sys::{JsString, Object};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "os")]
    extern "C" {
        #[wasm_bindgen(js_name = "EOL")]
        pub static EOL: Object;

        pub fn arch() -> JsString;
        pub fn homedir() -> JsString;
        pub fn machine() -> JsString;
        pub fn platform() -> JsString;
        pub fn tmpdir() -> JsString;
    }
}

#[cfg(test)]
mod test {
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn invoke_arch() {
        super::arch();
    }

    #[wasm_bindgen_test]
    fn invoke_homedir() {
        super::homedir();
    }

    #[wasm_bindgen_test]
    fn invoke_machine() {
        super::machine();
    }

    #[wasm_bindgen_test]
    fn invoke_platform() {
        super::platform();
    }

    #[wasm_bindgen_test]
    fn invoke_temp_dir() {
        super::temp_dir();
    }
}
