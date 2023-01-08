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

#[cfg(test)]
mod test {
    use super::*;
    use js_sys::Object;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn construct_writable() {
        let options = Object::default();
        ffi::Writable::new(Some(options));
    }
}
