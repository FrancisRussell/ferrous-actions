use super::path::{self, Path};
use std::collections::HashMap;
use wasm_bindgen::JsValue;

pub fn cwd() -> path::Path {
    path::Path::from(ffi::cwd())
}

pub fn get_env() -> HashMap<String, String> {
    use js_sys::JsString;
    use wasm_bindgen::JsCast as _;

    let env = &ffi::ENV;
    let env = js_sys::Object::entries(
        env.dyn_ref::<js_sys::Object>()
            .expect("get_env didn't return an object"),
    )
    .iter()
    .map(|o| o.dyn_into::<js_sys::Array>().expect("env entry was not an array"))
    .map(|a| (JsString::from(a.at(0)), JsString::from(a.at(1))))
    .map(|(k, v)| (String::from(k), String::from(v)))
    .collect();
    env
}

pub fn set_var(name: &str, value: &str) {
    use js_sys::{JsString, Map, Object};

    let name: JsString = name.into();
    let value: JsString = value.into();
    let attributes = Map::new();
    attributes.set(&"writable".into(), &true.into());
    attributes.set(&"enumerable".into(), &true.into());
    attributes.set(&"configurable".into(), &true.into());
    attributes.set(&"value".into(), value.as_ref());
    let attributes = Object::from_entries(&attributes).expect("Failed to convert attributes map to object");
    Object::define_property(&ffi::ENV, &name, &attributes);
}

pub fn remove_var(name: &str) {
    js_sys::Reflect::delete_property(&ffi::ENV, &name.into()).expect("process.env wasn't an object");
}

pub fn chdir<P: Into<Path>>(path: P) -> Result<(), JsValue> {
    let path = path.into();
    ffi::chdir(&path.to_js_string())?;
    Ok(())
}

pub mod ffi {
    use js_sys::{JsString, Object};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "process")]
    extern "C" {
        #[wasm_bindgen(js_name = "env")]
        pub static ENV: Object;

        pub fn cwd() -> JsString;

        #[wasm_bindgen(catch)]
        pub fn chdir(path: &JsString) -> Result<JsValue, JsValue>;
    }
}

#[cfg(test)]
mod test {
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn invoke_get_env() {
        super::get_env();
    }

    #[wasm_bindgen_test]
    async fn invoke_cwd() {
        let cwd = super::cwd();
        assert!(cwd.exists().await);
    }
}
