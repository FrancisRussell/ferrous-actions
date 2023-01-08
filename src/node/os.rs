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

pub fn temp_dir() -> path::Path {
    path::Path::from(ffi::tmpdir())
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
