use crate::node::stream;
use js_sys::{Function, Object};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsValue;

pub struct Sink {
    _write: Closure<dyn FnMut(JsValue, JsValue, JsValue)>,
    writable: stream::ffi::Writable,
}

impl Default for Sink {
    fn default() -> Sink {
        let write = Closure::new(|_chunk: JsValue, _encoding: JsValue, callback: JsValue| {
            let callback: Function = callback.into();
            let this = JsValue::null();
            let error = JsValue::null();
            callback.call1(&this, &error).expect("Error calling write callback");
        });
        let options = js_sys::Map::new();
        options.set(&"write".into(), write.as_ref());
        let options = Object::from_entries(&options).expect("Failed to convert options map to object");
        let writable = stream::ffi::Writable::new(Some(options));
        Sink {
            _write: write,
            writable,
        }
    }
}

impl AsRef<JsValue> for Sink {
    fn as_ref(&self) -> &JsValue {
        self.writable.as_ref()
    }
}
