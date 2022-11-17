pub mod ffi {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "noop-stream")]
    extern "C" {
        #[wasm_bindgen(js_name = "readableNoopStream")]
        pub fn readable() -> JsValue;

        #[wasm_bindgen(js_name = "writableNoopStream")]
        pub fn writable() -> JsValue;
    }
}
