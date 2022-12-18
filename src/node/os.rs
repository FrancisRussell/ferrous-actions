use super::path;

pub fn platform() -> String {
    ffi::platform().into()
}

pub fn machine() -> String {
    ffi::machine().into()
}

pub fn arch() -> String {
    ffi::arch().into()
}

pub fn homedir() -> path::Path {
    path::Path::from(ffi::homedir())
}

pub mod ffi {
    use js_sys::JsString;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "os")]
    extern "C" {
        pub fn arch() -> JsString;
        pub fn homedir() -> JsString;
        pub fn machine() -> JsString;
        pub fn platform() -> JsString;
    }
}
