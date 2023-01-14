use crate::node::path::Path;
use crate::{node, noop_stream};
use js_sys::{JsString, Object};
use parking_lot::Mutex;
use std::sync::Arc;
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

/// Work around for https://github.com/FrancisRussell/ferrous-actions-dev/issues/81
struct StreamToLines {
    buffer: Arc<Mutex<Vec<u8>>>,
    callback: Arc<Box<dyn Fn(&str)>>,
    closure: Closure<dyn Fn(JsValue)>,
}

impl StreamToLines {
    pub fn new(callback: Arc<Box<dyn Fn(&str)>>) -> StreamToLines {
        let buffer: Arc<Mutex<Vec<u8>>> = Arc::default();
        let closure = {
            let buffer = buffer.clone();
            let newline = node::os::eol();
            let callback = callback.clone();
            Closure::new(move |data: JsValue| {
                let data: js_sys::Uint8Array = data.into();
                let mut buffer = buffer.lock();
                let buffer_len = buffer.len();
                buffer.resize(buffer_len + data.length() as usize, 0u8);
                data.copy_to(&mut buffer[buffer_len..]);
                let mut offset = 0;
                while let Some(newline_pos) = Self::find_pattern_idx(&buffer[offset..], newline.as_bytes()) {
                    let line = String::from_utf8_lossy(&buffer[offset..(offset + newline_pos)]);
                    callback(&line);
                    offset += newline_pos + newline.as_bytes().len();
                }
                drop(buffer.drain(..offset));
            })
        };

        StreamToLines {
            buffer,
            callback,
            closure,
        }
    }

    fn find_pattern_idx(data: &[u8], pattern: &[u8]) -> Option<usize> {
        // We have this for str, surely it must exist in std for slice?
        if pattern.first().is_some() {
            let pattern_len = pattern.len();
            if pattern_len > data.len() {
                return None;
            }
            for idx in 0..=(data.len() - pattern.len()) {
                if &data[idx..(idx + pattern_len)] == pattern {
                    return Some(idx);
                }
            }
            None
        } else {
            Some(0)
        }
    }
}

impl Drop for StreamToLines {
    fn drop(&mut self) {
        let mut buffer = self.buffer.lock();
        let string = String::from_utf8_lossy(&buffer[..]);
        (self.callback)(&string);
        buffer.clear();
    }
}

impl AsRef<JsValue> for StreamToLines {
    fn as_ref(&self) -> &JsValue {
        self.closure.as_ref()
    }
}

pub struct Command {
    command: Path,
    args: Vec<JsString>,
    outline: Option<Arc<Box<dyn Fn(&str)>>>,
    errline: Option<Arc<Box<dyn Fn(&str)>>>,
    stdout: Stdio,
    stderr: Stdio,
    cwd: Path,
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
        let args: Vec<JsString> = self.args.iter().map(JsString::to_string).collect();
        let options = js_sys::Map::new();
        let listeners = js_sys::Map::new();

        let outline_adapter = self.outline.clone().map(StreamToLines::new);
        if let Some(callback) = &outline_adapter {
            listeners.set(&"stdout".into(), callback.as_ref());
        }

        let errline_adapter = self.errline.clone().map(StreamToLines::new);
        if let Some(callback) = &errline_adapter {
            listeners.set(&"stderr".into(), callback.as_ref());
        }

        options.set(&"cwd".into(), &self.cwd.to_js_string());
        let sink = noop_stream::Sink::default();
        if let StdioEnum::Null = self.stdout.inner {
            options.set(&"outStream".into(), sink.as_ref());
        }
        if let StdioEnum::Null = self.stderr.inner {
            options.set(&"errStream".into(), sink.as_ref());
        }
        let listeners = Object::from_entries(&listeners).expect("Failed to convert listeners map to object");
        options.set(&"listeners".into(), &listeners);
        let options = Object::from_entries(&options).expect("Failed to convert options map to object");
        ffi::exec(&command, Some(args), &options).await.map(|r| {
            #[allow(clippy::cast_possible_truncation)]
            let code = r.as_f64().expect("exec didn't return a number") as i32;
            code
        })
    }

    pub fn outline<F: Fn(&str) + 'static>(&mut self, callback: F) -> &mut Command {
        self.outline = Some(Arc::new(Box::new(callback)));
        self
    }

    pub fn errline<F: Fn(&str) + 'static>(&mut self, callback: F) -> &mut Command {
        self.errline = Some(Arc::new(Box::new(callback)));
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

    pub fn current_dir(&mut self, path: &Path) -> &mut Command {
        self.cwd = path.clone();
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
            cwd: node::process::cwd(),
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
