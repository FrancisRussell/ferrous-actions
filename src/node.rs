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
    use wasm_bindgen::JsValue;

    pub async fn chmod<P: Into<JsString>>(path: P, mode: u16) -> Result<(), JsValue> {
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

pub mod process {
    use std::collections::HashMap;

    pub fn env() -> HashMap<String, String> {
        let env = ffi::env();
        let env: HashMap<String, String> = serde_wasm_bindgen::from_value(env)
            .expect("Failed to deserialize environment variables");
        env
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(module = "process")]
        extern "C" {
            #[wasm_bindgen(getter)]
            pub fn env() -> JsValue;
        }
    }
}
