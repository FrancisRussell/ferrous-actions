use crate::node::path::Path;
use js_sys::JsString;
use js_sys::Object;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsValue;

pub struct Command {
    command: Path,
    args: Vec<JsString>,
    outline: Option<Closure<dyn Fn(JsString)>>,
    errline: Option<Closure<dyn Fn(JsString)>>,
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
        let command: JsString = (&self.command).into();
        let args: Vec<JsString> = self.args.iter().map(|a| a.to_string()).collect();
        let options = js_sys::Map::new();
        let listeners = js_sys::Map::new();
        if let Some(callback) = &self.outline {
            listeners.set(&"stdline".into(), callback.as_ref());
        }
        if let Some(callback) = &self.errline {
            listeners.set(&"errline".into(), callback.as_ref());
        }
        let listeners =
            Object::from_entries(&listeners).expect("Failed to convert listeners map to object");
        options.set(&"listeners".into(), &listeners);
        let options =
            Object::from_entries(&options).expect("Failed to convert options map to object");
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
}

impl<'a> From<&'a Path> for Command {
    fn from(path: &'a Path) -> Command {
        Command {
            command: path.clone(),
            args: Vec::new(),
            outline: None,
            errline: None,
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
