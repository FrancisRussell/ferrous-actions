use crate::node::path::Path;
use js_sys::JsString;
use std::borrow::Borrow;
use std::convert::Into;
use wasm_bindgen::prelude::*;

pub struct CacheEntry {
    key: JsString,
    paths: Vec<JsString>,
    restore_keys: Vec<JsString>,
}

impl CacheEntry {
    pub fn new<K: Into<JsString>>(key: K) -> CacheEntry {
        CacheEntry {
            key: key.into(),
            paths: Vec::new(),
            restore_keys: Vec::new(),
        }
    }

    pub fn paths<I: IntoIterator<Item = P>, P: Borrow<Path>>(
        &mut self,
        paths: I,
    ) -> &mut CacheEntry {
        self.paths
            .extend(paths.into_iter().map(|p| p.borrow().into()));
        self
    }

    pub fn path<P: Borrow<Path>>(&mut self, path: P) -> &mut CacheEntry {
        self.paths(std::iter::once(path.borrow()))
    }

    pub fn restore_keys<I, K>(&mut self, restore_keys: I) -> &mut CacheEntry
    where
        I: IntoIterator<Item = K>,
        K: Into<JsString>,
    {
        self.restore_keys
            .extend(restore_keys.into_iter().map(|k| k.into()));
        self
    }

    pub fn restore_key<K: Into<JsString>>(&mut self, restore_key: K) -> &mut CacheEntry {
        self.restore_keys(std::iter::once(restore_key.into()))
    }

    pub async fn save(&self) -> Result<i64, JsValue> {
        use wasm_bindgen::JsCast;
        let result = ffi::save_cache(self.paths.clone(), &self.key).await?;
        let result = result
            .dyn_ref::<js_sys::Number>()
            .ok_or_else(|| JsError::new("saveCache didn't return a number"))
            .map(|n| n.value_of() as i64)?;
        Ok(result)
    }

    pub async fn restore(&self) -> Result<Option<String>, JsValue> {
        let result =
            ffi::restore_cache(self.paths.clone(), &self.key, self.restore_keys.clone()).await?;
        if result == JsValue::UNDEFINED {
            Ok(None)
        } else {
            let result: JsString = result.into();
            Ok(Some(result.into()))
        }
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
