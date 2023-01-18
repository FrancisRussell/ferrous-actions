use super::scoped_cwd::ScopedCwd;
use crate::node;
use crate::node::path::Path;
use js_sys::JsString;
use std::collections::HashMap;
use std::convert::Into;
use wasm_bindgen::prelude::*;

pub struct Entry {
    key: JsString,
    paths: Vec<Path>,
    restore_keys: Vec<JsString>,
    cross_os_archive: bool,
}

impl Entry {
    pub fn new<K: Into<JsString>>(key: K) -> Entry {
        Entry {
            key: key.into(),
            paths: Vec::new(),
            restore_keys: Vec::new(),
            cross_os_archive: false,
        }
    }

    pub fn paths<I: IntoIterator<Item = P>, P: Into<Path>>(&mut self, paths: I) -> &mut Entry {
        self.paths.extend(paths.into_iter().map(|p| p.into()));
        self
    }

    pub fn path<P: Into<Path>>(&mut self, path: P) -> &mut Entry {
        self.paths(std::iter::once(path.into()))
    }

    pub fn permit_sharing_with_windows(&mut self, allow: bool) -> &mut Entry {
        self.cross_os_archive = allow;
        self
    }

    pub fn restore_keys<I, K>(&mut self, restore_keys: I) -> &mut Entry
    where
        I: IntoIterator<Item = K>,
        K: Into<JsString>,
    {
        self.restore_keys.extend(restore_keys.into_iter().map(Into::into));
        self
    }

    pub fn restore_key<K: Into<JsString>>(&mut self, restore_key: K) -> &mut Entry {
        self.restore_keys(std::iter::once(restore_key.into()))
    }

    pub async fn save(&self) -> Result<i64, JsValue> {
        use wasm_bindgen::JsCast;
        let cache_root = self.caching_path_root()?;
        let patterns = self.build_patterns(&cache_root);
        let result = {
            let _caching_scope = ScopedCwd::new(&cache_root)?;
            ffi::save_cache(patterns, &self.key, None, self.cross_os_archive).await?
        };
        let result = result
            .dyn_ref::<js_sys::Number>()
            .ok_or_else(|| JsError::new("saveCache didn't return a number"))
            .map(|n| {
                #[allow(clippy::cast_possible_truncation)]
                let id = n.value_of() as i64;
                id
            })?;
        Ok(result)
    }

    pub async fn save_if_update(&self, old_restore_key: Option<&str>) -> Result<Option<i64>, JsValue> {
        let new_restore_key = self.peek_restore().await?;
        if new_restore_key.is_none() || new_restore_key.as_deref() == old_restore_key {
            self.save().await.map(Some)
        } else {
            Ok(None)
        }
    }

    fn build_patterns(&self, relative_to: &Path) -> Vec<JsString> {
        let cwd = node::process::cwd();
        let mut result = Vec::with_capacity(self.paths.len());
        for path in self.paths.iter() {
            let absolute = cwd.join(path);
            let relative = absolute.relative_to(relative_to);
            result.push(relative.into());
        }
        result
    }

    fn caching_path_root(&self) -> Result<Path, JsValue> {
        Ok(node::os::homedir())
    }

    fn find_github_workspace() -> Result<Path, JsValue> {
        let env = node::process::get_env();
        let workspace = env
            .get("GITHUB_WORKSPACE")
            .ok_or_else(|| js_sys::Error::new("Unable to find GitHub workspace"))?;
        Ok(workspace.into())
    }

    fn caching_root_key(&self) -> Result<String, JsValue> {
        let root = self.caching_path_root()?;
        let workspace = Self::find_github_workspace()?;
        let relative = root.relative_to(workspace);
        Ok(relative.to_string())
    }

    pub async fn restore(&self) -> Result<Option<String>, JsValue> {
        crate::info!("Restoring the following paths: {:#?}", self.paths);
        crate::info!("The environment: {:#?}", crate::node::process::get_env());
        let cache_root = self.caching_path_root()?;
        let patterns = self.build_patterns(&cache_root);
        crate::info!("Restoring the following patterns: {:#?}", patterns);
        let result = {
            let _caching_scope = ScopedCwd::new(&cache_root)?;
            ffi::restore_cache(
                patterns,
                &self.key,
                self.restore_keys.clone(),
                None,
                self.cross_os_archive,
            )
            .await?
        };
        if result == JsValue::NULL || result == JsValue::UNDEFINED {
            Ok(None)
        } else {
            let result: JsString = result.into();
            Ok(Some(result.into()))
        }
    }

    async fn peek_restore(&self) -> Result<Option<String>, JsValue> {
        use js_sys::Object;

        let compression_method: JsString = ffi::get_compression_method().await?.into();
        let keys: Vec<JsString> = std::iter::once(&self.key)
            .chain(self.restore_keys.iter())
            .cloned()
            .collect();
        let options = {
            let options = js_sys::Map::new();
            options.set(&"compressionMethod".into(), &compression_method.into());
            options.set(&"enableCrossOsArchive".into(), &self.cross_os_archive.into());
            Object::from_entries(&options).expect("Failed to convert options map to object")
        };
        let cache_root = self.caching_path_root()?;
        let patterns = self.build_patterns(&cache_root);
        let result = {
            let _caching_scope = ScopedCwd::new(&cache_root)?;
            ffi::get_cache_entry(keys, patterns, Some(options)).await?
        };
        if result == JsValue::NULL || result == JsValue::UNDEFINED {
            Ok(None)
        } else {
            let result: Object = result.into();
            let entries = Object::entries(&result);
            let mut entries: HashMap<String, JsValue> = entries
                .iter()
                .map(Into::<js_sys::Array>::into)
                .map(|e| (e.get(0), e.get(1)))
                .map(|(k, v)| (Into::<JsString>::into(k), v))
                .map(|(k, v)| (Into::<String>::into(k), v))
                .collect();
            Ok(entries
                .remove("cacheKey")
                .map(Into::<JsString>::into)
                .map(Into::<String>::into))
        }
    }
}

pub mod ffi {
    use js_sys::{JsString, Object};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "@actions/cache")]
    extern "C" {
        #[wasm_bindgen(js_name = "saveCache", catch)]
        pub async fn save_cache(
            paths: Vec<JsString>,
            key: &JsString,
            upload_options: Option<Object>,
            cross_os_archive: bool,
        ) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(js_name = "restoreCache", catch)]
        pub async fn restore_cache(
            paths: Vec<JsString>,
            primary_key: &JsString,
            restore_keys: Vec<JsString>,
            download_options: Option<Object>,
            cross_os_archive: bool,
        ) -> Result<JsValue, JsValue>;
    }

    #[wasm_bindgen(module = "@actions/cache/lib/internal/cacheUtils")]
    extern "C" {
        #[wasm_bindgen(js_name = "getCompressionMethod", catch)]
        pub(super) async fn get_compression_method() -> Result<JsValue, JsValue>;
    }

    #[wasm_bindgen(module = "@actions/cache/lib/internal/cacheHttpClient")]
    extern "C" {
        #[wasm_bindgen(js_name = "getCacheEntry", catch)]
        pub(super) async fn get_cache_entry(
            keys: Vec<JsString>,
            paths: Vec<JsString>,
            options: Option<Object>,
        ) -> Result<JsValue, JsValue>;
    }
}
