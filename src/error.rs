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
}

impl From<JsValue> for Error {
    fn from(value: JsValue) -> Error {
        Error::Js(value)
    }
}
