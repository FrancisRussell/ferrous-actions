use crate::node::path::Path;
use js_sys::JsString;
use std::convert::Into;
use wasm_bindgen::prelude::*;

pub async fn save_cache(paths: &[Path], key: &str) -> Result<i64, JsValue> {
    use wasm_bindgen::JsCast;

    let paths = paths.iter().map(Into::<JsString>::into).collect();
    let key: JsString = key.into();
    let result = ffi::save_cache(paths, &key).await?;
    let result = result
        .dyn_ref::<js_sys::Number>()
        .ok_or_else(|| JsError::new("saveCache didn't return a number"))
        .map(|n| n.value_of() as i64)?;
    Ok(result)
}

pub mod ffi {
    use js_sys::JsString;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "@actions/cache")]
    extern "C" {
        #[wasm_bindgen(js_name = "saveCache", catch)]
        pub async fn save_cache(paths: Vec<JsString>, key: &JsString) -> Result<JsValue, JsValue>;
    }
}
