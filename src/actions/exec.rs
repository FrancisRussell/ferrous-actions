use crate::info;
use crate::node::path::Path;
use js_sys::JsString;
use js_sys::Object;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsValue;

pub async fn exec<S: Into<JsString>, A: IntoIterator<Item = S>>(
    command_line: &Path,
    args: A,
) -> Result<i32, JsValue> {
    let command_line: JsString = command_line.into();
    let args: Vec<JsString> = args.into_iter().map(|a| a.into()).collect();
    let options = js_sys::Map::new();
    let listeners = js_sys::Map::new();
    let closure_o: Closure<dyn Fn(JsValue)> =
        Closure::new(|value| info!("stdout got a value: {:?}", value));
    let closure_e: Closure<dyn Fn(JsValue)> =
        Closure::new(|value| info!("stderr got a value: {:?}", value));
    listeners.set(&"stdline".into(), closure_o.as_ref());
    listeners.set(&"errline".into(), closure_e.as_ref());
    let listeners =
        Object::from_entries(&listeners).expect("Failed to convert listeners map to object");
    info!("Listeners set to: {:?}", listeners.to_string());
    options.set(&"listeners".into(), &listeners);
    let options = Object::from_entries(&options).expect("Failed to convert options map to object");
    info!("Options set to: {:?}", options.to_string());
    ffi::exec(&command_line, Some(args), &options)
        .await
        .map(|r| r.as_f64().expect("exec didn't return a number") as i32)
}

pub mod ffi {
    use js_sys::JsString;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "@actions/exec")]
    extern "C" {
        #[wasm_bindgen(catch)]
        pub async fn exec(
            comand_line: &JsString,
            args: Option<Vec<JsString>>,
            options: &JsValue,
        ) -> Result<JsValue, JsValue>;
    }
}
