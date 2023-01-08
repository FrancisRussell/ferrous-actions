pub mod ffi {
    use js_sys::Object;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "stream")]
    extern "C" {
        #[derive(Debug)]
        #[wasm_bindgen(js_name = "Writable")]
        pub type Writable;

        #[wasm_bindgen(constructor)]
        pub fn new(options: Option<Object>) -> Writable;
    }
}
