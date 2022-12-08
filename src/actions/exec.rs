use crate::node::path::Path;
use crate::noop_stream;
use js_sys::{JsString, Object};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsValue;

#[derive(Debug, Clone, Copy)]
enum StdioEnum {
    Inherit,
    Null,
}

#[derive(Debug, Clone, Copy)]
pub struct Stdio {
    inner: StdioEnum,
}

impl Stdio {
    pub fn null() -> Stdio {
        Stdio { inner: StdioEnum::Null }
    }

    pub fn inherit() -> Stdio {
        Stdio {
            inner: StdioEnum::Inherit,
        }
    }
}

pub struct Command {
    command: Path,
    args: Vec<JsString>,
    outline: Option<Closure<dyn Fn(JsString)>>,
    errline: Option<Closure<dyn Fn(JsString)>>,
    stdout: Stdio,
    stderr: Stdio,
}

impl Command {
    pub fn args<I, S>(&mut self, args: I) -> &mut Command
    where
        I: IntoIterator<Item = S>,
        S: Into<JsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn arg<S: Into<JsString>>(&mut self, arg: S) -> &mut Command {
        self.args(std::iter::once(arg.into()));
        self
    }

    pub async fn exec(&mut self) -> Result<i32, JsValue> {
        let command = self.command.to_string();
        let command = Self::escape_command(command.as_str());
        let command: JsString = command.into();
        let args: Vec<JsString> = self.args.iter().map(|a| a.to_string()).collect();
        let options = js_sys::Map::new();
        let listeners = js_sys::Map::new();
        if let Some(callback) = &self.outline {
            listeners.set(&"stdline".into(), callback.as_ref());
        }
        if let Some(callback) = &self.errline {
            listeners.set(&"errline".into(), callback.as_ref());
        }
        if let StdioEnum::Null = self.stdout.inner {
            options.set(&"outStream".into(), &noop_stream::ffi::writable());
        }
        if let StdioEnum::Null = self.stderr.inner {
            options.set(&"errStream".into(), &noop_stream::ffi::writable());
        }
        let listeners = Object::from_entries(&listeners).expect("Failed to convert listeners map to object");
        options.set(&"listeners".into(), &listeners);
        let options = Object::from_entries(&options).expect("Failed to convert options map to object");
        ffi::exec(&command, Some(args), &options)
            .await
            .map(|r| r.as_f64().expect("exec didn't return a number") as i32)
    }

    pub fn outline<F: Fn(&str) + 'static>(&mut self, callback: F) -> &mut Command {
        self.outline = Some(Closure::new(move |line: JsString| {
            let line: String = line.into();
            callback(line.as_str());
        }));
        self
    }

    pub fn errline<F: Fn(&str) + 'static>(&mut self, callback: F) -> &mut Command {
        self.errline = Some(Closure::new(move |line: JsString| {
            let line: String = line.into();
            callback(line.as_str());
        }));
        self
    }

    pub fn stdout(&mut self, redirect: Stdio) -> &mut Command {
        self.stdout = redirect;
        self
    }

    pub fn stderr(&mut self, redirect: Stdio) -> &mut Command {
        self.stderr = redirect;
        self
    }

    // Some bright spark had the idea of making an exec function that could both
    // handle execvp and shell command style invocations rather than have two
    // functions or some sort of flag to handle these different use cases.
    // Consequently we now need to escape our command so the apparently bespoke
    // unescaping strategy in `argStringToArray` will not mangle our command
    // in the case it contains spaces or double quotes.
    fn escape_command(command: &str) -> String {
        let mut result = String::with_capacity(command.len());
        // - Spaces must be located between quotes to not be considered a token
        //   separator.
        // - Outside of double quotes backslash is itself.
        // - Within double quotes, backslash is itself unless followed by a double quote
        //   in which case it is the double quote. This means double quotes cannot
        //   surround a string-fragment containing a trailing backslash.
        for c in command.chars() {
            match c {
                ' ' => result.push_str("\" \""),
                '\"' => result.push_str("\"\\\""),
                _ => result.push(c),
            }
        }
        result
    }
}

impl<'a> From<&'a Path> for Command {
    fn from(path: &'a Path) -> Command {
        Command {
            command: path.clone(),
            args: Vec::new(),
            outline: None,
            errline: None,
            stdout: Stdio::inherit(),
            stderr: Stdio::inherit(),
        }
    }
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
