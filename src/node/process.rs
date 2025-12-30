use super::path::{self, Path};
use std::collections::HashMap;
use wasm_bindgen::JsValue;

/// Returns the current working directory of the process
pub fn cwd() -> path::Path {
    path::Path::from(ffi::cwd())
}

/// Returns a map of all environment variables defined for the process
pub fn get_env() -> HashMap<String, String> {
    use js_sys::JsString;
    use wasm_bindgen::JsCast as _;

    let env = ffi::ENV.with(|v| {
        js_sys::Object::entries(v.dyn_ref::<js_sys::Object>().expect("get_env didn't return an object"))
            .iter()
            .map(|o| o.dyn_into::<js_sys::Array>().expect("env entry was not an array"))
            .map(|a| (JsString::from(a.at(0)), JsString::from(a.at(1))))
            .map(|(k, v)| (String::from(k), String::from(v)))
            .collect()
    });
    env
}

/// Set an environment variable to a specified value
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
    ffi::ENV.with(|env| Object::define_property(env, &name, &attributes));
}

/// Removes an environment variable
pub fn remove_var(name: &str) {
    ffi::ENV.with(|env| js_sys::Reflect::delete_property(env, &name.into()).expect("process.env wasn't an object"));
}

/// Changes the current working directory to the specified path
pub fn chdir<P: Into<Path>>(path: P) -> Result<(), JsValue> {
    let path = path.into();
    ffi::chdir(&path.to_js_string())?;
    Ok(())
}

/// Low-level bindings for node.js process functions and variables
pub mod ffi {
    use js_sys::{JsString, Object};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "process")]
    extern "C" {
        #[wasm_bindgen(thread_local_v2, js_name = "env")]
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
