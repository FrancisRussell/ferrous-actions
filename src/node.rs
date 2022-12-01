pub mod os {
    use super::path;

    pub fn platform() -> String {
        ffi::platform().into()
    }

    pub fn homedir() -> path::Path {
        path::Path::from(ffi::homedir())
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(module = "os")]
        extern "C" {
            pub fn platform() -> JsString;
            pub fn homedir() -> JsString;
        }
    }
}

pub mod fs {
    use js_sys::{JsString, Uint8Array};
    use wasm_bindgen::{JsCast, JsError, JsValue};

    pub async fn chmod<P: Into<JsString>>(path: P, mode: u16) -> Result<(), JsValue> {
        let path: JsString = path.into();
        ffi::chmod(&path, mode).await.map(|_| ())
    }

    pub async fn read_file<P: Into<JsString>>(path: P) -> Result<Vec<u8>, JsValue> {
        let path: JsString = path.into();
        let buffer = ffi::read_file(&path).await?;
        let buffer = buffer
            .dyn_ref::<Uint8Array>()
            .ok_or_else(|| JsError::new("readFile didn't return an array"))?;
        let length = buffer.length();
        let mut result = vec![0u8; length as usize];
        buffer.copy_to(&mut result);
        Ok(result)
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsValue;

        #[wasm_bindgen(module = "fs/promises")]
        extern "C" {
            #[wasm_bindgen(catch)]
            pub async fn chmod(path: &JsString, mode: u16) -> Result<JsValue, JsValue>;

            #[wasm_bindgen(catch, js_name = "readFile")]
            pub async fn read_file(path: &JsString) -> Result<JsValue, JsValue>;
        }
    }
}

pub mod path {
    use js_sys::JsString;

    pub struct Path {
        inner: JsString,
    }

    impl std::fmt::Display for Path {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
            let string = String::from(&self.inner);
            formatter.write_str(string.as_str())
        }
    }

    impl Path {
        pub fn push<P: Into<Path>>(&mut self, path: P) {
            let path = path.into();
            let joined = ffi::resolve(vec![self.inner.to_string(), path.inner.to_string()]);
            self.inner = joined;
        }

        pub fn to_js_string(&self) -> JsString {
            self.inner.to_string()
        }
    }

    impl std::fmt::Debug for Path {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
            write!(formatter, "{}", self)
        }
    }

    impl Clone for Path {
        fn clone(&self) -> Path {
            Path {
                inner: self.inner.to_string(),
            }
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

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(module = "path")]
        extern "C" {
            pub fn normalize(path: &JsString) -> JsString;
            #[wasm_bindgen(variadic)]
            pub fn join(paths: Vec<JsString>) -> JsString;
            #[wasm_bindgen(variadic)]
            pub fn resolve(paths: Vec<JsString>) -> JsString;
        }
    }
}

pub mod process {
    use super::path;

    pub fn cwd() -> path::Path {
        path::Path::from(ffi::cwd())
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(module = "process")]
        extern "C" {
            pub fn cwd() -> JsString;
        }
    }
}
