use js_sys::JsString;
use wasm_bindgen::JsValue;

pub async fn exec<C: Into<JsString>, S: Into<JsString>, A: IntoIterator<Item = S>>(
    command_line: C,
    args: A,
) -> Result<i32, JsValue> {
    let command_line: JsString = command_line.into();
    let args: Vec<JsString> = args.into_iter().map(|a| a.into()).collect();
    ffi::exec(&command_line, Some(args), None)
        .await
        .map(|r| r.as_f64().expect("exec didn't return a number") as i32)
}

pub mod ffi {
    use js_sys::JsString;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "@actions/exec")]
    extern "C" {
        pub type ExecOptions;

        #[wasm_bindgen(catch)]
        pub async fn exec(
            comand_line: &JsString,
            args: Option<Vec<JsString>>,
            options: Option<&ExecOptions>,
        ) -> Result<JsValue, JsValue>;
    }
}
