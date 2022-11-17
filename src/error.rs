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
}
