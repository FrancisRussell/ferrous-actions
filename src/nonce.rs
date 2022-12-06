use rust_toolchain_manifest::HashValue;

pub fn build_nonce(num_bytes: usize) -> HashValue {
    if num_bytes > 32 {
        panic!("Maximum nonce size is 32 bytes");
    }
    // To use the getrandom crate we either need node.js's crypto module to be enabled, or
    // switch to WASI as a target
    let mut hasher = blake3::Hasher::new();
    // We need to introduce 256 bits of entropy regardless of the number of bytes we take.
    // Using a hasher lets us ignore some potential issues with Math.random():
    // https://v8.dev/blog/math-random.
    let num_floats_to_input = 8;
    for _ in 0..num_floats_to_input {
        let random = ffi::MATH
            .random()
            .as_f64()
            .expect("Math.random() didn't return a float");
        let random = random * ((1u64 << f64::MANTISSA_DIGITS) as f64);
        let random = random as u64;
        hasher.update(&random.to_le_bytes());
    }
    let hash = hasher.finalize();
    let hash = HashValue::from_bytes(&hash.as_bytes()[..num_bytes]);
    hash
}

mod ffi {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        pub type MathObject;

        #[wasm_bindgen(js_name = "Math")]
        pub static MATH: MathObject;

        #[wasm_bindgen(method)]
        pub fn random(this: &MathObject) -> JsValue;
    }
}
