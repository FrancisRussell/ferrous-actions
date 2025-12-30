use crate::node::path::Path;
use crate::node::process;
use js_sys::JsString;
use std::borrow::Cow;
use std::convert::Into;
use wasm_bindgen::prelude::*;

/// Builder for a tool downloader
#[derive(Debug)]
pub struct DownloadTool {
    url: JsString,
    dest: Option<Path>,
    auth: Option<JsString>,
}

impl<U: Into<JsString>> From<U> for DownloadTool {
    /// Constructs a `DownloadTool` that will download from the specified URL
    fn from(url: U) -> DownloadTool {
        DownloadTool {
            url: url.into(),
            dest: None,
            auth: None,
        }
    }
}

impl DownloadTool {
    /// Set the destination path of the download
    pub fn dest<D: Into<Path>>(&mut self, dest: D) -> &mut Self {
        self.dest = Some(dest.into());
        self
    }

    /// Specify an authorization header
    pub fn auth<A: Into<JsString>>(&mut self, auth: A) -> &mut Self {
        self.auth = Some(auth.into());
        self
    }

    /// Perform the download and return the path the file was downloaded to
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

/// Downloads a tool from a specified URL
pub async fn download_tool<O: Into<DownloadTool>>(options: O) -> Result<Path, JsValue> {
    options.into().download().await
}

/// Different types of compression that may be applied to a tar file
#[derive(Debug, Copy, Clone)]
pub enum StreamCompression {
    /// None
    None,

    /// Gzip
    Gzip,

    /// Bzip2
    Bzip2,

    /// LZMA
    Xz,
}

impl StreamCompression {
    fn tar_flag(&self) -> Cow<'_, str> {
        match self {
            StreamCompression::None => "",
            StreamCompression::Gzip => "z",
            StreamCompression::Bzip2 => "j",
            StreamCompression::Xz => "J",
        }
        .into()
    }
}

/// Extracts a tar file with the specified compression. An output directory can
/// be optionally specified.
pub async fn extract_tar(path: &Path, compression: StreamCompression, dest: Option<&Path>) -> Result<Path, JsValue> {
    let mut tar_option = String::from("x");
    tar_option += &compression.tar_flag();
    let tar_option = vec![JsString::from(tar_option)];

    let path: JsString = path.into();
    let dest = dest.map(Into::<JsString>::into);
    let dest = ffi::extract_tar(&path, dest.as_ref(), Some(tar_option)).await?;
    let dest: JsString = dest.into();
    Ok(dest.into())
}

/// Saves a path into a local cache
pub async fn cache_dir(tool: &str, version: &str, path: &Path, arch: Option<&str>) -> Result<Path, JsValue> {
    let path: JsString = path.into();
    let tool: JsString = tool.into();
    let version: JsString = version.into();
    let arch: Option<JsString> = arch.map(Into::into);
    let dest = ffi::cache_dir(&path, &tool, &version, arch.as_ref()).await?;
    let dest: JsString = dest.into();
    Ok(dest.into())
}

/// Low level bindings for the GitHub Actions Toolkit "tool cache" API
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

        #[wasm_bindgen(js_name = "cacheDir", catch)]
        pub async fn cache_dir(
            source_dir: &JsString,
            tool: &JsString,
            version: &JsString,
            arch: Option<&JsString>,
        ) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(js_name = "extractTar", catch)]
        pub async fn extract_tar(
            file: &JsString,
            dest: Option<&JsString>,
            flags: Option<Vec<JsString>>,
        ) -> Result<JsValue, JsValue>;
    }
}
