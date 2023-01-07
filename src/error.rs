use crate::package_manifest;
use thiserror::Error;
use wasm_bindgen::JsValue;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0:?}")]
    Js(JsValue),

    #[error("Unable to parse option `{0}`, which was supplied as `{1}`")]
    OptionParseError(String, String),

    #[error("Unable to parse as an argument list: `{0}`")]
    ArgumentsParseError(String),

    #[error("Unknown command: {0}")]
    UnknownCommand(String),

    #[error("Toolchain parse error: {0}")]
    ToolchainParse(#[from] rust_toolchain_manifest::toolchain::ToolchainParseError),

    #[error("Manifest file not UTF-8")]
    ManifestNotUtf8,

    #[error("Manifest error: {0}")]
    ManifestError(#[from] rust_toolchain_manifest::Error),

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("Unable to parse package manifest: {0}")]
    PackageManifest(#[from] package_manifest::ParseError),

    #[error("JSON serialization/deserialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Unable to parse item to cache: {0}")]
    ParseCacheableItem(String),

    #[error("Unable to parse cross-plaform-sharing: {0}")]
    ParseCrossPlatformSharing(String),

    #[error("Unable to parse duration: {0}")]
    DurationParse(#[from] humantime::DurationError),

    #[error("Out of range duration: {0}")]
    OutOfRangeDuration(#[from] chrono::OutOfRangeError),

    #[error("Path does not exist: {0}")]
    PathDoesNotExist(String),

    #[error("Error during path match construction: {0}")]
    PathMatchPatternError(#[from] simple_path_match::PatternError),

    #[error("Required input was not supplied: {0}")]
    MissingInput(String),

    #[error("Toolchain install backend does not support {0} functionality")]
    ToolchainInstallFunctionality(String),
}

impl From<JsValue> for Error {
    fn from(value: JsValue) -> Error {
        Error::Js(value)
    }
}
