use crate::node::path::Path;
use js_sys::JsString;
use std::borrow::Borrow;
use std::convert::Into;
use wasm_bindgen::prelude::*;

pub async fn save_cache<P, K>(paths: &[P], key: K) -> Result<i64, JsValue>
where
    P: Borrow<Path>,
    K: AsRef<str>,
{
    use wasm_bindgen::JsCast;

    let paths = paths
        .iter()
        .map(Borrow::borrow)
        .map(Into::<JsString>::into)
        .collect();
    let key: JsString = key.as_ref().into();
    let result = ffi::save_cache(paths, &key).await?;
    let result = result
        .dyn_ref::<js_sys::Number>()
        .ok_or_else(|| JsError::new("saveCache didn't return a number"))
        .map(|n| n.value_of() as i64)?;
    Ok(result)
}

pub async fn restore_cache<P, K, R>(
    paths: &[P],
    primary_key: K,
    restore_keys: &[R],
) -> Result<Option<String>, JsValue>
where
    P: Borrow<Path>,
    K: AsRef<str>,
    R: AsRef<str>,
{
    let paths: Vec<JsString> = paths
        .iter()
        .map(Borrow::borrow)
        .map(Into::<JsString>::into)
        .collect();
    let primary_key: JsString = primary_key.as_ref().into();
    let restore_keys: Vec<JsString> = restore_keys
        .iter()
        .map(AsRef::as_ref)
        .map(Into::<JsString>::into)
        .collect();
    let result = ffi::restore_cache(paths, &primary_key, restore_keys).await?;
    if result == JsValue::UNDEFINED {
        Ok(None)
    } else {
        let result: JsString = result.into();
        Ok(Some(result.into()))
    }
}

pub mod ffi {
    use js_sys::JsString;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "@actions/cache")]
    extern "C" {
        #[wasm_bindgen(js_name = "saveCache", catch)]
        pub async fn save_cache(paths: Vec<JsString>, key: &JsString) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(js_name = "restoreCache", catch)]
        pub async fn restore_cache(
            paths: Vec<JsString>,
            primary_key: &JsString,
            restore_keys: Vec<JsString>,
        ) -> Result<JsValue, JsValue>;
    }
}
