use super::path::Path;
use futures::channel::oneshot;
use futures::future::{FutureExt as _, Shared};
use js_sys::{JsString, Object};
use parking_lot::Mutex;
use std::borrow::Cow;
use std::pin::Pin;
use std::sync::Arc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast as _, JsValue};

#[derive(Debug, Clone, Copy)]
enum StdioEnum {
    Ignore,
    Inherit,
    Piped,
}

impl StdioEnum {
    fn nodejs_name(self) -> Cow<'static, str> {
        match self {
            StdioEnum::Inherit => "inherit",
            StdioEnum::Ignore => "ignore",
            StdioEnum::Piped => "piped",
        }
        .into()
    }
}

/// Where output of a standard stream can be redirected
#[derive(Debug, Clone, Copy)]
pub struct Stdio {
    inner: StdioEnum,
}

impl Stdio {
    /// Constructs a `Stdio` which causes output to be discarded
    pub fn ignore() -> Stdio {
        Stdio {
            inner: StdioEnum::Ignore,
        }
    }

    /// Constructs a `Stdio` which causes output to be send to the same location
    /// as it would for the parent process
    pub fn inherit() -> Stdio {
        Stdio {
            inner: StdioEnum::Inherit,
        }
    }
}

/// Builder for executing a command
pub struct Command {
    path: Path,
    args: Vec<JsString>,
    cwd: Option<Path>,
    stdout: Stdio,
    stderr: Stdio,
}

impl<'a> From<&'a Path> for Command {
    /// Constructs a command that will execute the file at the specified path.
    fn from(path: &'a Path) -> Command {
        Command {
            path: path.clone(),
            args: Vec::new(),
            cwd: None,
            stdout: Stdio::inherit(),
            stderr: Stdio::inherit(),
        }
    }
}

#[derive(Default)]
struct ChildStateMutable {
    result: Option<Result<ExitStatus, JsValue>>,
}

struct ChildState {
    subprocess: Object,
    error_callback: Closure<dyn FnMut(JsValue)>,
    close_callback: Closure<dyn FnMut(JsValue, JsValue)>,
    mutable: Arc<Mutex<ChildStateMutable>>,
    #[allow(clippy::type_complexity)]
    completion: Shared<Pin<Box<dyn futures::Future<Output = Result<ExitStatus, JsValue>>>>>,
}

pub struct Child {
    state: Arc<ChildState>,
}

impl Child {
    pub fn wait(&self) -> impl futures::Future<Output = Result<ExitStatus, JsValue>> {
        self.state.completion.clone()
    }
}

#[derive(Debug, Clone)]
pub enum ExitStatus {
    /// The process exited with this exit code.
    Code(i32),

    /// The process was terminated by a signal.
    Signal(String),
}

impl Command {
    /// Specified additional command arguments
    pub fn args<I, S>(&mut self, args: I) -> &mut Command
    where
        I: IntoIterator<Item = S>,
        S: Into<JsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Specify a command argument
    pub fn arg<S: Into<JsString>>(&mut self, arg: S) -> &mut Command {
        self.args(std::iter::once(arg.into()));
        self
    }

    pub fn spawn(&self) -> Result<Child, JsValue> {
        let child_state_mutable: Arc<Mutex<ChildStateMutable>> = Arc::default();

        // Handler for spawn errors
        let child_state_mutable_error = child_state_mutable.clone();
        let error_callback = Closure::once(move |error: JsValue| {
            let mut guard = child_state_mutable_error.lock();
            guard.result.get_or_insert(Result::<ExitStatus, JsValue>::Err(error));
        });

        // Handler for spawned process termination
        let (close_sender, close_receiver) = oneshot::channel();
        let child_state_mutable_close = child_state_mutable.clone();
        let close_callback = Closure::once(move |code: JsValue, signal: JsValue| {
            let result = if let Some(code) = code.as_f64() {
                #[allow(clippy::cast_possible_truncation)]
                ExitStatus::Code(code as i32)
            } else if let Some(signal) = signal.as_string() {
                ExitStatus::Signal(signal)
            } else {
                unreachable!("Child process did not have an exit code and wasn't killed by a signal");
            };
            let mut guard = child_state_mutable_close.lock();
            guard.result.get_or_insert(Result::<ExitStatus, JsValue>::Ok(result));
            let _ = close_sender.send(());
        });

        let options = js_sys::Map::new();
        if let Some(cwd) = &self.cwd {
            options.set(&"cwd".into(), &cwd.to_js_string());
        }
        let stdio: js_sys::Array = [Stdio::ignore(), self.stdout, self.stderr]
            .into_iter()
            .map(|s| JsString::from(s.inner.nodejs_name().as_ref()))
            .collect();
        options.set(&"stdio".into(), stdio.as_ref());
        let options = Object::from_entries(&options).expect("Failed to convert options map to object");

        let subprocess = ffi::spawn(&self.path.to_js_string(), self.args.clone(), options)?;
        // Register callbacks
        let on_fn = js_sys::Reflect::get(&subprocess, &"on".into())?.dyn_into::<js_sys::Function>()?;
        on_fn.call2(&subprocess, &"error".into(), error_callback.as_ref())?;
        on_fn.call2(&subprocess, &"close".into(), close_callback.as_ref())?;

        let child_state_mutable_completion = child_state_mutable.clone();
        let completion = close_receiver.map(move |_| {
            let guard = child_state_mutable_completion.lock();
            guard.result.clone().expect("Spawn result unexpectedly missing")
        });
        let completion: Pin<Box<dyn futures::Future<Output = Result<ExitStatus, JsValue>>>> = Box::pin(completion);

        let child_state = ChildState {
            subprocess,
            error_callback,
            close_callback,
            mutable: child_state_mutable,
            completion: completion.shared(),
        };
        Ok(Child {
            state: Arc::new(child_state),
        })
    }

    /// Sets the current working directory of the command
    pub fn current_dir(&mut self, path: &Path) -> &mut Command {
        self.cwd = Some(path.clone());
        self
    }
}

/// Low-level bindings for node.js process functions and variables
pub mod ffi {
    use js_sys::{JsString, Object};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "child_process")]
    extern "C" {
        #[wasm_bindgen(catch)]
        pub fn spawn(command: &JsString, args: Vec<JsString>, options: Object) -> Result<Object, JsValue>;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::info;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    async fn invoke_spawn() {
        let mut command = Command::from(&Path::from("ls"));
        command.arg("non-existent").current_dir(&Path::from("/tmp"));
        let child = command.spawn().expect("Spawn failure");
        info!("Spawn result was: {:?}", child.wait().await);
    }
}
