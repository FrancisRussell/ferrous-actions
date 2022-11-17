use crate::node::path::Path;
use crate::node::process;
use js_sys::JsString;
use std::convert::Into;
use wasm_bindgen::prelude::*;

#[derive(Debug)]
pub struct DownloadTool {
    url: JsString,
    dest: Option<Path>,
    auth: Option<JsString>,
}

impl<U: Into<JsString>> From<U> for DownloadTool {
    fn from(url: U) -> DownloadTool {
        DownloadTool {
            url: url.into(),
            dest: None,
            auth: None,
        }
    }
}

impl DownloadTool {
    pub fn dest<D: Into<Path>>(&mut self, dest: D) -> &mut Self {
        self.dest = Some(dest.into());
        self
    }

    pub fn auth<A: Into<JsString>>(&mut self, auth: A) -> &mut Self {
        self.auth = Some(auth.into());
        self
    }

    pub async fn download(&mut self) -> Result<Path, JsValue> {
        let dest = self.dest.as_ref().map(|dest| {
            let mut resolved = process::cwd();
            resolved.push(dest.clone());
            JsString::from(&resolved)
        });
        ffi::download_tool(&self.url, dest.as_ref(), self.auth.as_ref(), None)
            .await
            .map(Into::<JsString>::into)
            .map(Into::<Path>::into)
    }
}

pub async fn download_tool<O: Into<DownloadTool>>(options: O) -> Result<Path, JsValue> {
    options.into().download().await
}

pub mod ffi {
    use js_sys::{JsString, Map};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "@actions/tool-cache")]
    extern "C" {
        #[wasm_bindgen(js_name = "downloadTool", catch)]
        pub async fn download_tool(
            url: &JsString,
            dest: Option<&JsString>,
            auth: Option<&JsString>,
            headers: Option<&Map>,
        ) -> Result<JsValue, JsValue>;
    }
}
