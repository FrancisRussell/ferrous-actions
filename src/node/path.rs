use js_sys::JsString;

#[derive(Clone)]
pub struct Path {
    inner: JsString,
}

impl std::fmt::Display for Path {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let string = String::from(&self.inner);
        formatter.write_str(string.as_str())
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
}

impl std::fmt::Debug for Path {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(formatter, "{}", self)
    }
}

impl From<JsString> for Path {
    fn from(path: JsString) -> Path {
        let path = ffi::normalize(&path);
        Path { inner: path }
    }
}

impl From<&str> for Path {
    fn from(path: &str) -> Path {
        let path: JsString = path.into();
        let path = ffi::normalize(&path);
        Path { inner: path }
    }
}

impl From<&Path> for JsString {
    fn from(path: &Path) -> JsString {
        path.inner.to_string()
    }
}

pub fn delimiter() -> String {
    use wasm_bindgen::JsCast as _;
    ffi::DELIMITER
        .clone()
        .dyn_into::<JsString>()
        .expect("delimiter wasn't a string")
        .into()
}

pub fn separator() -> String {
    use wasm_bindgen::JsCast as _;
    ffi::SEPARATOR
        .clone()
        .dyn_into::<JsString>()
        .expect("separator wasn't a string")
        .into()
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
