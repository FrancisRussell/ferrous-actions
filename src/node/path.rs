use js_sys::JsString;
use lazy_static::lazy_static;
use std::borrow::Cow;

#[derive(Clone)]
pub struct Path {
    inner: JsString,
}

lazy_static! {
    static ref SEPARATOR: String = {
        use wasm_bindgen::JsCast as _;
        ffi::SEPARATOR
            .clone()
            .dyn_into::<JsString>()
            .expect("separator wasn't a string")
            .into()
    };
}

lazy_static! {
    static ref DELIMITER: String = {
        use wasm_bindgen::JsCast as _;
        ffi::DELIMITER
            .clone()
            .dyn_into::<JsString>()
            .expect("delimiter wasn't a string")
            .into()
    };
}

impl std::fmt::Display for Path {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let string = String::from(&self.inner);
        string.fmt(formatter)
    }
}

impl PartialEq for Path {
    fn eq(&self, rhs: &Path) -> bool {
        // relative() resolves paths according to the CWD so we should only
        // use it if they will both be resolved the same way
        if self.is_absolute() == rhs.is_absolute() {
            // This should handle both case-sensitivity and trailing slash issues
            let relative = ffi::relative(&self.inner, &rhs.inner);
            relative.length() == 0
        } else {
            false
        }
    }
}

impl Path {
    pub fn push<P: Into<Path>>(&mut self, path: P) {
        let path = path.into();
        let joined = if path.is_absolute() {
            path.inner
        } else {
            ffi::join(vec![self.inner.clone(), path.inner])
        };
        self.inner = joined;
    }

    pub fn to_js_string(&self) -> JsString {
        self.inner.to_string()
    }

    #[must_use]
    pub fn parent(&self) -> Path {
        let parent = ffi::dirname(&self.inner);
        Path { inner: parent }
    }

    pub fn is_absolute(&self) -> bool {
        ffi::is_absolute(&self.inner)
    }

    pub fn file_name(&self) -> String {
        let result = ffi::basename(&self.inner, None);
        result.into()
    }

    pub async fn exists(&self) -> bool {
        super::fs::ffi::access(&self.inner, None).await.is_ok()
    }

    #[must_use]
    pub fn join<P: Into<Path>>(&self, path: P) -> Path {
        let mut result = self.clone();
        result.push(path.into());
        result
    }

    pub fn relative_to<P: Into<Path>>(&self, path: P) -> Path {
        let path = path.into();
        let relative = ffi::relative(&path.inner, &self.inner);
        if relative.length() == 0 {
            ".".into()
        } else {
            relative.into()
        }
    }
}

impl std::fmt::Debug for Path {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(formatter, "{}", self)
    }
}

impl From<&JsString> for Path {
    fn from(path: &JsString) -> Path {
        let path = ffi::normalize(path);
        Path { inner: path }
    }
}

impl From<JsString> for Path {
    fn from(path: JsString) -> Path {
        Path::from(&path)
    }
}

impl From<&Path> for Path {
    fn from(path: &Path) -> Path {
        path.clone()
    }
}

impl From<&str> for Path {
    fn from(path: &str) -> Path {
        let path: JsString = path.into();
        let path = ffi::normalize(&path);
        Path { inner: path }
    }
}

impl From<&String> for Path {
    fn from(path: &String) -> Path {
        Path::from(path.as_str())
    }
}

impl From<Path> for JsString {
    fn from(path: Path) -> JsString {
        path.inner
    }
}

impl From<&Path> for JsString {
    fn from(path: &Path) -> JsString {
        path.inner.clone()
    }
}

pub fn delimiter() -> Cow<'static, str> {
    DELIMITER.as_str().into()
}

pub fn separator() -> Cow<'static, str> {
    SEPARATOR.as_str().into()
}

pub mod ffi {
    use js_sys::{JsString, Object};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "path")]
    extern "C" {
        #[wasm_bindgen(js_name = "delimiter")]
        pub static DELIMITER: Object;

        #[wasm_bindgen(js_name = "sep")]
        pub static SEPARATOR: Object;

        pub fn normalize(path: &JsString) -> JsString;
        #[wasm_bindgen(variadic)]
        pub fn join(paths: Vec<JsString>) -> JsString;
        #[wasm_bindgen(variadic)]
        pub fn resolve(paths: Vec<JsString>) -> JsString;
        #[wasm_bindgen]
        pub fn dirname(path: &JsString) -> JsString;
        #[wasm_bindgen(js_name = "isAbsolute")]
        pub fn is_absolute(path: &JsString) -> bool;
        #[wasm_bindgen]
        pub fn relative(from: &JsString, to: &JsString) -> JsString;
        #[wasm_bindgen]
        pub fn basename(path: &JsString, suffix: Option<JsString>) -> JsString;
    }
}

#[cfg(test)]
mod test {
    use super::Path;
    use crate::node;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn check_absolute() {
        let cwd = node::process::cwd();
        assert!(cwd.is_absolute());
    }

    #[wasm_bindgen_test]
    fn check_relative() {
        let relative = Path::from(&format!("{}{}{}", "a", super::separator(), "b"));
        assert!(!relative.is_absolute());
    }

    #[wasm_bindgen_test]
    fn check_separator() {
        let separator = super::separator();
        assert!(separator == "/" || separator == "\\");
    }

    #[wasm_bindgen_test]
    fn check_delimiter() {
        let delimiter = super::delimiter();
        assert!(delimiter == ";" || delimiter == ":");
    }

    #[wasm_bindgen_test]
    fn check_parent() {
        let parent_name = "parent";
        let path = Path::from(&format!("{}{}{}", parent_name, super::separator(), "child"));
        let parent_path = path.parent();
        assert_eq!(parent_path.to_string(), parent_name);
    }

    #[wasm_bindgen_test]
    fn check_basename() {
        let child_base = "child.";
        let child_ext = ".extension";
        let child_name = format!("{}{}", child_base, child_ext);
        let path = Path::from(&format!("{}{}{}", "parent", super::separator(), child_name));
        assert_eq!(child_name, path.file_name());
        assert_eq!(
            child_name,
            String::from(super::ffi::basename(&path.to_js_string(), None))
        );
        assert_eq!(
            child_name,
            String::from(super::ffi::basename(&path.to_js_string(), Some(".nomatch".into())))
        );
        assert_eq!(
            child_base,
            String::from(super::ffi::basename(&path.to_js_string(), Some(child_ext.into())))
        );
    }

    #[wasm_bindgen_test]
    fn check_push() {
        let parent_name = "a";
        let child_name = "b";
        let path_string = format!("{}{}{}", parent_name, super::separator(), child_name);
        let mut path = Path::from(parent_name);
        path.push(child_name);
        assert_eq!(path.to_string(), path_string);
    }

    #[wasm_bindgen_test]
    fn check_join() {
        let parent_name = "a";
        let child_name = "b";
        let path_string = format!("{}{}{}", parent_name, super::separator(), child_name);
        let path = Path::from(parent_name).join(child_name);
        assert_eq!(path.to_string(), path_string);
    }

    #[wasm_bindgen_test]
    fn check_current_normalization() {
        use itertools::Itertools as _;
        let current = ".";
        let long_current = std::iter::repeat(current).take(10).join(&super::separator());
        assert_eq!(Path::from(&long_current).to_string(), current);
    }

    #[wasm_bindgen_test]
    fn check_parent_normalization() {
        use itertools::Itertools as _;
        let parent = "..";
        let current = ".";
        let count = 10;

        let long_current = std::iter::repeat("child")
            .take(count)
            .chain(std::iter::repeat(parent).take(count))
            .join(&super::separator());
        assert_eq!(Path::from(&long_current).to_string(), current);

        let long_parent = std::iter::repeat("child")
            .take(count)
            .chain(std::iter::repeat(parent).take(count + 1))
            .join(&super::separator());
        assert_eq!(Path::from(&long_parent).to_string(), parent);
    }

    #[wasm_bindgen_test]
    async fn check_exists() -> Result<(), JsValue> {
        let temp = node::os::temp_dir();
        let file_name = format!("ferrous-actions-exists-test - {}", chrono::Local::now());
        let temp_file_path = temp.join(&file_name);
        let data = "Nothing to see here\n";
        node::fs::write_file(&temp_file_path, data.as_bytes()).await?;
        assert!(temp_file_path.exists().await);
        node::fs::remove_file(&temp_file_path).await?;
        assert!(!temp_file_path.exists().await);
        Ok(())
    }

    #[wasm_bindgen_test]
    fn check_equality() {
        use itertools::Itertools as _;

        // We can't check case behaviour without knowing filesystem semantics.
        // It's unclear if a trailing slash matters equality-wise.

        assert_eq!(Path::from("a"), Path::from("a"));
        assert_eq!(Path::from("."), Path::from("."));
        assert_eq!(Path::from(".."), Path::from(".."));
        assert_eq!(
            Path::from(&format!("a{}..", super::separator())),
            Path::from(&format!("b{}..", super::separator()))
        );
        assert_ne!(Path::from("."), Path::from(".."));
        assert_ne!(Path::from("a"), Path::from("b"));

        let path = ["a", "b", "c", "d"].into_iter().join(&super::separator());
        assert_eq!(Path::from(&path), Path::from(&path));
    }
}
