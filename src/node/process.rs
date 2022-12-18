use super::path;
use std::collections::HashMap;

pub fn cwd() -> path::Path {
    path::Path::from(ffi::cwd())
}

pub fn get_env() -> HashMap<String, String> {
    use js_sys::JsString;
    use wasm_bindgen::JsCast as _;

    let env = ffi::ENV.clone();
    let env = js_sys::Object::entries(
        &env.dyn_into::<js_sys::Object>()
            .expect("get_env didn't return an object"),
    )
    .iter()
    .map(|o| o.dyn_into::<js_sys::Array>().expect("env entry was not an array"))
    .map(|a| (JsString::from(a.at(0)), JsString::from(a.at(1))))
    .map(|(k, v)| (String::from(k), String::from(v)))
    .collect();
    env
}

pub mod ffi {
    use js_sys::{JsString, Object};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "process")]
    extern "C" {
        #[wasm_bindgen(js_name = "env")]
        pub static ENV: Object;

        pub fn cwd() -> JsString;
    }
}
