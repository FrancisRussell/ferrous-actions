use crate::node::path::Path;
use js_sys::JsString;

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        $crate::actions::core::info(std::format!($($arg)*).as_str());
    }};
}

pub fn info<S: Into<JsString>>(message: S) {
    ffi::info(&message.into());
}

pub fn set_output<N: Into<JsString>, V: Into<JsString>>(name: N, value: V) {
    ffi::set_output(&name.into(), &value.into())
}

#[derive(Debug)]
pub struct Input {
    name: JsString,
    required: bool,
    trim_whitespace: bool,
}

impl<N: Into<JsString>> From<N> for Input {
    fn from(name: N) -> Input {
        Input {
            name: name.into(),
            required: false,
            trim_whitespace: true,
        }
    }
}

impl Input {
    pub fn required(&mut self, value: bool) -> &mut Input {
        self.required = value;
        self
    }

    pub fn trim_whitespace(&mut self, value: bool) -> &mut Input {
        self.trim_whitespace = value;
        self
    }

    fn to_ffi(&self) -> ffi::InputOptions {
        ffi::InputOptions {
            required: Some(self.required),
            trim_whitespace: Some(self.trim_whitespace),
        }
    }

    pub fn get(&mut self) -> Option<String> {
        let ffi = self.to_ffi();
        let value = String::from(ffi::get_input(&self.name, Some(ffi)));
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    pub fn get_required(&mut self) -> String {
        let mut ffi = self.to_ffi();
        ffi.required = Some(true);
        String::from(ffi::get_input(&self.name, Some(ffi)))
    }
}

#[derive(Debug)]
pub struct Annotation {
    message: String,
    title: Option<String>,
    file: Option<Path>,
    start_line: Option<usize>,
    end_line: Option<usize>,
    start_column: Option<usize>,
    end_column: Option<usize>,
}

impl<M: Into<String>> From<M> for Annotation {
    fn from(message: M) -> Annotation {
        Annotation {
            message: message.into(),
            title: None,
            file: None,
            start_line: None,
            end_line: None,
            start_column: None,
            end_column: None,
        }
    }
}

impl Annotation {
    pub fn title(&mut self, title: &str) -> &mut Annotation {
        self.title = Some(title.to_string());
        self
    }

    pub fn file(&mut self, path: &Path) -> &mut Annotation {
        self.file = Some(path.clone());
        self
    }

    pub fn start_line(&mut self, start_line: usize) -> &mut Annotation {
        self.start_line = Some(start_line);
        self
    }

    pub fn end_line(&mut self, end_line: usize) -> &mut Annotation {
        self.end_line = Some(end_line);
        self
    }

    pub fn start_column(&mut self, start_column: usize) -> &mut Annotation {
        self.start_column = Some(start_column);
        self
    }

    pub fn end_column(&mut self, end_column: usize) -> &mut Annotation {
        self.end_column = Some(end_column);
        self
    }
}

pub fn get_input<I: Into<Input>>(input: I) -> Option<String> {
    let mut input = input.into();
    input.get()
}

pub fn set_failed<M: Into<JsString>>(message: M) {
    ffi::set_failed(&message.into())
}

pub fn add_path(path: &Path) {
    ffi::add_path(&path.into())
}

#[allow(clippy::drop_non_drop)]
pub mod ffi {
    use js_sys::JsString;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct InputOptions {
        pub required: Option<bool>,

        #[wasm_bindgen(js_name = "trimWhitespace")]
        pub trim_whitespace: Option<bool>,
    }

    #[wasm_bindgen(module = "@actions/core")]
    extern "C" {
        /// Gets the value of an input. The value is also trimmed.
        #[wasm_bindgen(js_name = "getInput")]
        pub fn get_input(name: &JsString, options: Option<InputOptions>) -> JsString;

        /// Writes info to log with console.log.
        #[wasm_bindgen]
        pub fn info(message: &JsString);

        /// Writes debug to log with console.log.
        #[wasm_bindgen]
        pub fn debug(message: &JsString);

        /// Sets the action status to failed.
        /// When the action exits it will be with an exit code of 1.
        #[wasm_bindgen(js_name = "setFailed")]
        pub fn set_failed(message: &JsString);

        /// Sets the value of an output.
        #[wasm_bindgen(js_name = "setOutput")]
        pub fn set_output(name: &JsString, value: &JsString);

        #[wasm_bindgen(js_name = "addPath")]
        pub fn add_path(path: &JsString);

    }
}
