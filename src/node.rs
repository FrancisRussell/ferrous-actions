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
    use js_sys::JsString;
    use wasm_bindgen::JsValue;

    pub async fn chmod<P: Into<JsString>>(path: P, mode: u16) -> Result<(), JsValue> {
        let path: JsString = path.into();
        ffi::chmod(&path, mode).await.map(|_| ())
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsValue;

        #[wasm_bindgen(module = "fs/promises")]
        extern "C" {
            #[wasm_bindgen(catch)]
            pub async fn chmod(path: &JsString, mode: u16) -> Result<JsValue, JsValue>;
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
        pub fn push<S: Into<JsString>>(&mut self, segment: S) {
            let joined = ffi::join(vec![self.inner.to_string(), segment.into()]);
            self.inner = joined;
        }
    }

    impl std::fmt::Debug for Path {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
            write!(formatter, "{}", self.to_string())
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
        }
    }
}
