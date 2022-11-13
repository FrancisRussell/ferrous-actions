pub mod os {
    pub fn platform() -> String {
        ffi::platform().into()
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(module = "os")]
        extern "C" {
            pub fn platform() -> JsString;
        }
    }
}

pub mod fs {
    use js_sys::JsString;
    use std::path::Path;
    use wasm_bindgen::JsValue;

    pub async fn chmod<P: AsRef<Path>>(path: P, mode: u16) -> Result<(), JsValue> {
        let path = path.as_ref();
        let path: String = path.to_string_lossy().into();
        let path: JsString = path.into();
        ffi::chmod(&path, mode).await.map(|_| ())
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsValue;

        #[wasm_bindgen(module = "fs/promises")]
        extern "C" {
            #[wasm_bindgen(catch)]
            pub async fn chmod(path: &JsString, mode: u16) -> Result<JsValue, JsValue>;
        }
    }
}
