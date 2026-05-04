use crate::node::path::Path;
use js_sys::{JsString, Number, Object};
use wasm_bindgen::JsValue;

/// Outputs a GitHub actions log line at debug level
pub fn debug<S: Into<JsString>>(message: S) {
    ffi::debug(&message.into());
}

/// Outputs a GitHub actions log line at info level
pub fn info<S: Into<JsString>>(message: S) {
    ffi::info(&message.into());
}

/// Outputs a GitHub actions log line at notice level (this will be
/// an annotation)
pub fn notice<A: Into<Annotation>>(message: A) {
    message.into().notice();
}

/// Outputs a GitHub actions log line at warning level (this will be
/// an annotation)
pub fn warning<A: Into<Annotation>>(message: A) {
    message.into().warning();
}

/// Formats and outputs a GitHub actions log line at error level (this will be
/// an annotation)
pub fn error<A: Into<Annotation>>(message: A) {
    message.into().error();
}

/// Sets a named action output to the specified value
pub fn set_output<N: Into<JsString>, V: Into<JsString>>(name: N, value: V) {
    ffi::set_output(&name.into(), &value.into());
}

/// Builder for retrieving action inputs
#[derive(Debug)]
pub struct Input {
    name: JsString,
    required: bool,
    trim_whitespace: bool,
}

impl<N: Into<JsString>> From<N> for Input {
    /// Construct a builder to access the specified input name
    fn from(name: N) -> Input {
        Input {
            name: name.into(),
            required: false,
            trim_whitespace: true,
        }
    }
}

impl Input {
    /// Mark this input as required. The `Input` will return an error on
    /// retrieval if the input is not defined.
    pub fn required(&mut self, value: bool) -> &mut Input {
        self.required = value;
        self
    }

    /// Specifies that whitespace should be trimmed from the retrieved input
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

    /// Gets the specified input (if defined)
    pub fn get(&mut self) -> Result<Option<String>, JsValue> {
        let ffi = self.to_ffi();
        let value = String::from(ffi::get_input(&self.name, Some(ffi))?);
        Ok(if value.is_empty() { None } else { Some(value) })
    }

    /// Gets the specified input, returning an error if it is not defined
    pub fn get_required(&mut self) -> Result<String, JsValue> {
        let mut ffi = self.to_ffi();
        ffi.required = Some(true);
        ffi::get_input(&self.name, Some(ffi)).map(String::from)
    }
}

/// Builder for outputting annotations
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
    /// Constructs an annotation with the specified message
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

/// Annotation levels
#[derive(Copy, Clone, Debug)]
pub enum AnnotationLevel {
    /// Notice
    Notice,

    /// Warning
    Warning,

    /// Error
    Error,
}

impl Annotation {
    /// Sets the title of the annotation
    pub fn title(&mut self, title: &str) -> &mut Annotation {
        self.title = Some(title.to_string());
        self
    }

    /// Sets the path to a file to which the annotation is relevant
    pub fn file(&mut self, path: &Path) -> &mut Annotation {
        self.file = Some(path.clone());
        self
    }

    /// Sets the line in the file the annotation should start
    pub fn start_line(&mut self, start_line: usize) -> &mut Annotation {
        self.start_line = Some(start_line);
        self
    }

    /// Sets the line in the file the annotation should end
    pub fn end_line(&mut self, end_line: usize) -> &mut Annotation {
        self.end_line = Some(end_line);
        self
    }

    /// Sets the column in the file the annotation should start
    pub fn start_column(&mut self, start_column: usize) -> &mut Annotation {
        self.start_column = Some(start_column);
        self
    }

    /// Sets the column in the file the annotation should end
    pub fn end_column(&mut self, end_column: usize) -> &mut Annotation {
        self.end_column = Some(end_column);
        self
    }

    fn build_js_properties(&self) -> Object {
        let properties = js_sys::Map::new();
        if let Some(title) = &self.title {
            properties.set(&"title".into(), JsString::from(title.as_str()).as_ref());
        }
        if let Some(file) = &self.file {
            properties.set(&"file".into(), file.to_js_string().as_ref());
        }
        for (name, value) in [
            ("startLine", &self.start_line),
            ("endLine", &self.end_line),
            ("startColumn", &self.start_column),
            ("endColumn", &self.end_column),
        ] {
            if let Some(number) = value.and_then(|n| TryInto::<u32>::try_into(n).ok()) {
                properties.set(&name.into(), Number::from(number).as_ref());
            }
        }
        Object::from_entries(&properties).expect("Failed to convert options map to object")
    }

    /// Outputs the annotation as an error
    pub fn error(&self) {
        self.output(AnnotationLevel::Error);
    }

    /// Outputs the annotation at notice level
    pub fn notice(&self) {
        self.output(AnnotationLevel::Notice);
    }

    /// Outputs the annotation as a warning
    pub fn warning(&self) {
        self.output(AnnotationLevel::Warning);
    }

    /// Outputs the annotation at the specified level
    pub fn output(&self, level: AnnotationLevel) {
        let message = JsString::from(self.message.as_str());
        let properties = self.build_js_properties();
        match level {
            AnnotationLevel::Error => ffi::error(&message, Some(properties)),
            AnnotationLevel::Warning => ffi::warning(&message, Some(properties)),
            AnnotationLevel::Notice => ffi::notice(&message, Some(properties)),
        }
    }
}

/// Retrieves the action input of the specified name
pub fn get_input<I: Into<Input>>(input: I) -> Result<Option<String>, JsValue> {
    let mut input = input.into();
    input.get()
}

/// Mark this action as failed for the specified reason
pub fn set_failed<M: Into<JsString>>(message: M) {
    ffi::set_failed(&message.into());
}

/// Adds the specified path into `$PATH` for use by later actions
pub fn add_path(path: &Path) {
    ffi::add_path(&path.into());
}

/// Exports an environment variable from the action
pub fn export_variable<N: Into<JsString>, V: Into<JsString>>(name: N, value: V) {
    let name = name.into();
    let value = value.into();
    ffi::export_variable(&name, &value);
}

/// Saves state for use by the action in a later phase
pub fn save_state<N: Into<JsString>, V: Into<JsString>>(name: N, value: V) {
    let name = name.into();
    let value = value.into();
    ffi::save_state(&name, &value);
}

/// Retrieves previously saved action state
pub fn get_state<N: Into<JsString>>(name: N) -> Option<String> {
    let name = name.into();
    let value: String = ffi::get_state(&name).into();
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.into())
    }
}

/// Starts a foldable group
pub fn start_group<N: Into<JsString>>(name: N) {
    ffi::start_group(&name.into());
}

/// Ends a foldable group
pub fn end_group() {
    ffi::end_group();
}

/// Low-level bindings to the GitHub Actions Toolkit "core" API
#[allow(clippy::drop_non_drop)]
pub mod ffi {
    use js_sys::{JsString, Object};
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
        #[wasm_bindgen(js_name = "getInput", catch)]
        pub fn get_input(name: &JsString, options: Option<InputOptions>) -> Result<JsString, JsValue>;

        /// Writes info
        #[wasm_bindgen]
        pub fn info(message: &JsString);

        /// Writes debug
        #[wasm_bindgen]
        pub fn debug(message: &JsString);

        /// Writes an error with an optional annotation
        #[wasm_bindgen]
        pub fn error(message: &JsString, annotation: Option<Object>);

        /// Writes a warning with an optional annotation
        #[wasm_bindgen]
        pub fn warning(message: &JsString, annotation: Option<Object>);

        /// Writes a notice with an optional annotation
        #[wasm_bindgen]
        pub fn notice(message: &JsString, annotation: Option<Object>);

        /// Sets the action status to failed.
        /// When the action exits it will be with an exit code of 1.
        #[wasm_bindgen(js_name = "setFailed")]
        pub fn set_failed(message: &JsString);

        /// Sets the value of an output.
        #[wasm_bindgen(js_name = "setOutput")]
        pub fn set_output(name: &JsString, value: &JsString);

        #[wasm_bindgen(js_name = "addPath")]
        pub fn add_path(path: &JsString);

        #[wasm_bindgen(js_name = "exportVariable")]
        pub fn export_variable(name: &JsString, value: &JsString);

        #[wasm_bindgen(js_name = "saveState")]
        pub fn save_state(name: &JsString, value: &JsString);

        #[wasm_bindgen(js_name = "getState")]
        pub fn get_state(name: &JsString) -> JsString;

        #[wasm_bindgen(js_name = "startGroup")]
        pub fn start_group(name: &JsString);

        #[wasm_bindgen(js_name = "endGroup")]
        pub fn end_group();
    }
}
