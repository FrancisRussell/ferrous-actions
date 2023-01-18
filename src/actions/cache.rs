use crate::node;
use crate::node::path::Path;
use js_sys::JsString;
use std::collections::HashMap;
use std::convert::Into;
use wasm_bindgen::prelude::*;

const WORKSPACE_ENV_VAR: &str = "GITHUB_WORKSPACE";

// Actually getting caching to work cross platform is complicated. First of all,
// the action takes patterns not paths (which is unhelpful for apps that don't
// want to use globs), It also means that on Windows you're going to need to
// convert any paths to use forward slash as a separator.
//
// The cache action keys actions on patterns, but the key is simply the hash of
// the patterns passed in, without modification. This means that for cross
// platform caching, the patterns need to be for relative paths, not absolute
// ones. This in turn means that if your action needs to produce a consistent
// cache key for a path that's not at a consistent location relative to the CWD
// at the time of action invocation, you're going to need to change CWD.
//
// As a final complication, when files are archived, they are done so using
// paths which are specified relative to the GitHub workspace. So, even if you
// sit in a directory you want to restore things in, and generate the right
// relative paths to match a cache key, you might end up restoring to the wrong
// location (e.g. because $CARGO_HOME moved relative to the GitHub workspace).
//
// The issue with any sort of reusable file caching across OSes is that there
// needs to be some concept of a reference path or paths which are well defined
// on each platform and under which it is valid to cache and restore certain
// paths. GitHub actions chooses this to be $GITHUB_WORKSPACE. Unfortunately
// this is problematic for two reasons:
// - We have no guarantee that the path we want to cache (e.g. something in the
//   home directory) will remain at consistent path relative to
//   $GITHUB_WORKSPACE (or that is is on other OSes).
// - Patterns cannot contain `.` or `..`, meaning we cannot use the GitHub
//   workspace as our root location when we want to cache paths located in the
//   home directory.
//
// To work around this, we have the cache user specify a root path. `Entry` both
// changes CWD to that path and rewrites the supplied paths to be relative to
// the root path. In addition, it sets $GITHUB_WORKSPACE to this path too, which
// causes all files in the generated tarball to be specified relative to that
// location. This is a hack, but in general it means that we can reliably cache
// and restore paths to locations that may change across time.

#[derive(Debug)]
pub struct ScopedWorkspace {
    original_cwd: Path,
    original_workspace: Option<String>,
}

impl ScopedWorkspace {
    pub fn new(new_cwd: &Path) -> Result<ScopedWorkspace, JsValue> {
        let original_cwd = node::process::cwd();
        let original_workspace = node::process::get_env().get(WORKSPACE_ENV_VAR).cloned();
        node::process::chdir(new_cwd)?;
        node::process::set_var(WORKSPACE_ENV_VAR, &new_cwd.to_string());
        Ok(ScopedWorkspace {
            original_cwd,
            original_workspace,
        })
    }
}

impl Drop for ScopedWorkspace {
    fn drop(&mut self) {
        if let Some(original_workspace) = self.original_workspace.as_deref() {
            node::process::set_var(WORKSPACE_ENV_VAR, original_workspace);
        } else {
            node::process::remove_var(WORKSPACE_ENV_VAR);
        }
        node::process::chdir(&self.original_cwd)
            .unwrap_or_else(|e| panic!("Unable to chdir back to original folder: {:?}", e));
    }
}

pub struct Entry {
    key: JsString,
    paths: Vec<Path>,
    restore_keys: Vec<JsString>,
    cross_os_archive: bool,
    relative_to: Option<Path>,
}

impl Entry {
    pub fn new<K: Into<JsString>>(key: K) -> Entry {
        Entry {
            key: key.into(),
            paths: Vec::new(),
            restore_keys: Vec::new(),
            cross_os_archive: false,
            relative_to: None,
        }
    }

    pub fn paths<I: IntoIterator<Item = P>, P: Into<Path>>(&mut self, paths: I) -> &mut Entry {
        self.paths.extend(paths.into_iter().map(|p| p.into()));
        self
    }

    pub fn path<P: Into<Path>>(&mut self, path: P) -> &mut Entry {
        self.paths(std::iter::once(path.into()))
    }

    pub fn root<P: Into<Path>>(&mut self, path: P) -> &mut Entry {
        self.relative_to = Some(path.into());
        self
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
        let patterns = self.build_patterns();
        let result = {
            let _caching_scope = self.build_action_scope()?;
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

    fn build_patterns(&self) -> Vec<JsString> {
        let cwd = node::process::cwd();
        let mut result = Vec::with_capacity(self.paths.len());
        for path in self.paths.iter() {
            let pattern = if let Some(relative_to) = &self.relative_to {
                let absolute = cwd.join(path);
                let relative = absolute.relative_to(relative_to);
                relative
            } else {
                path.clone()
            };
            result.push(pattern.into());
        }
        result
    }

    fn build_action_scope(&self) -> Result<Option<ScopedWorkspace>, JsValue> {
        self.relative_to.as_ref().map(ScopedWorkspace::new).transpose()
    }

    pub async fn restore(&self) -> Result<Option<String>, JsValue> {
        crate::info!("Restoring the following paths: {:#?}", self.paths);
        crate::info!("The environment: {:#?}", crate::node::process::get_env());
        let patterns = self.build_patterns();
        crate::info!("Restoring the following patterns: {:#?}", patterns);
        let result = {
            let _caching_scope = self.build_action_scope()?;
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
        let patterns = self.build_patterns();
        let result = {
            let _caching_scope = self.build_action_scope()?;
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
