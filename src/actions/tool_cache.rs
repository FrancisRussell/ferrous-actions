use js_sys::JsString;
use std::convert::Into;
use wasm_bindgen::prelude::*;

#[derive(Default)]
pub struct DownloadParams {
    dest: Option<JsString>,
    auth: Option<JsString>,
}

pub async fn download_tool<U: Into<JsString>>(
    url: U,
    params: &DownloadParams,
) -> Result<String, JsValue> {
    let url = url.into();
    ffi::download_tool(&url, params.dest.as_ref(), params.auth.as_ref(), None)
        .await
        .map(Into::<JsString>::into)
        .map(Into::<String>::into)
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
