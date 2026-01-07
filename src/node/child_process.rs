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

pub trait Direction {
    type Callback;
}

pub struct Source;

impl Direction for Source {
    type Callback = Box<dyn FnMut(Option<&[u8]>)>;
}

enum StdioEnum<D: Direction> {
    Ignore,
    Inherit,
    Piped(D::Callback),
}

impl<D: Direction> StdioEnum<D> {
    fn nodejs_name(&self) -> Cow<'static, str> {
        match self {
            StdioEnum::Inherit => "inherit",
            StdioEnum::Ignore => "ignore",
            StdioEnum::Piped(_) => "pipe",
        }
        .into()
    }
}

/// Where output of a standard stream can be redirected
pub struct Stdio<D: Direction> {
    inner: StdioEnum<D>,
}

impl<D: Direction> Stdio<D> {
    /// Constructs a `Stdio` which causes output to be discarded
    pub fn ignore() -> Stdio<D> {
        Stdio {
            inner: StdioEnum::Ignore,
        }
    }

    /// Constructs a `Stdio` which causes output to be send to the same location
    /// as it would for the parent process
    pub fn inherit() -> Stdio<D> {
        Stdio {
            inner: StdioEnum::Inherit,
        }
    }

    /// Constructs a `Stdio` which causes output to be sent or received by the
    /// specified callback.
    pub fn piped(callback: D::Callback) -> Stdio<D> {
        Stdio {
            inner: StdioEnum::Piped(callback),
        }
    }
}

/// Builder for executing a command
pub struct Command {
    path: Path,
    args: Vec<JsString>,
    cwd: Option<Path>,
    stdout: Stdio<Source>,
    stderr: Stdio<Source>,
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
    new_data_callbacks: Vec<Closure<dyn FnMut(JsValue)>>,
    stream_closed_callbacks: Vec<Closure<dyn Fn()>>,
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

    pub fn spawn(&mut self) -> Result<Child, JsValue> {
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
        let stdio: js_sys::Array = [&Stdio::ignore(), &self.stdout, &self.stderr]
            .into_iter()
            .map(|s| JsString::from(s.inner.nodejs_name().as_ref()))
            .collect();
        options.set(&"stdio".into(), stdio.as_ref());
        let options = Object::from_entries(&options).expect("Failed to convert options map to object");

        let subprocess = ffi::spawn(&self.path.to_js_string(), self.args.clone(), options)?;
        // Register callbacks
        let on_fn_subprocess = js_sys::Reflect::get(&subprocess, &"on".into())?.dyn_into::<js_sys::Function>()?;
        on_fn_subprocess.call2(&subprocess, &"error".into(), error_callback.as_ref())?;
        on_fn_subprocess.call2(&subprocess, &"close".into(), close_callback.as_ref())?;

        let child_state_mutable_completion = child_state_mutable.clone();
        let completion = close_receiver.map(move |_| {
            let guard = child_state_mutable_completion.lock();
            guard.result.clone().expect("Spawn result unexpectedly missing")
        });
        let completion: Pin<Box<dyn futures::Future<Output = Result<ExitStatus, JsValue>>>> = Box::pin(completion);

        let mut new_data_callbacks = Vec::new();
        let mut stream_closed_callbacks = Vec::new();

        for (name, mode) in [("stdout", &mut self.stdout.inner), ("stderr", &mut self.stderr.inner)] {
            if matches!(mode, StdioEnum::Piped(_)) {
                // The child takes ownership of any callbacks so stdio/stderr are reset to
                // inherit if callbacks have been set for them.
                let mut stdio = StdioEnum::Inherit;
                std::mem::swap(mode, &mut stdio);
                let mut buffer = Vec::new();
                if let StdioEnum::Piped(callback) = stdio {
                    let callback_new_data = Arc::new(Mutex::new(callback));
                    let callback_end_of_stream = callback_new_data.clone();
                    let new_data_closure: Closure<dyn FnMut(JsValue)> = Closure::new(move |data: JsValue| {
                        let data: js_sys::Uint8Array = data.into();
                        let num_bytes = usize::try_from(data.length()).expect("Array too large");
                        if num_bytes > buffer.len() {
                            buffer.resize(num_bytes, 0u8);
                        }
                        data.copy_to(&mut buffer[..num_bytes]);
                        callback_new_data.lock()(Some(&buffer[..num_bytes]));
                    });
                    let stream_closed_closure: Closure<dyn Fn()> = Closure::new(move || {
                        callback_end_of_stream.lock()(None);
                    });

                    let stream = js_sys::Reflect::get(&subprocess, &name.into())?.dyn_into::<js_sys::Object>()?;
                    let on_fn_stream = js_sys::Reflect::get(&stream, &"on".into())?.dyn_into::<js_sys::Function>()?;
                    on_fn_stream.call2(&stream, &"data".into(), new_data_closure.as_ref())?;
                    on_fn_stream.call2(&stream, &"close".into(), stream_closed_closure.as_ref())?;
                    new_data_callbacks.push(new_data_closure);
                    stream_closed_callbacks.push(stream_closed_closure);
                }
            }
        }

        let child_state = ChildState {
            subprocess,
            error_callback,
            close_callback,
            mutable: child_state_mutable,
            completion: completion.shared(),
            new_data_callbacks,
            stream_closed_callbacks,
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

    /// Sets where standard error should be directed
    pub fn stderr(&mut self, redirect: Stdio<Source>) -> &mut Command {
        self.stderr = redirect;
        self
    }

    /// Sets where standard output should be directed
    pub fn stdout(&mut self, redirect: Stdio<Source>) -> &mut Command {
        self.stdout = redirect;
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
        let stdout_callback: Box<dyn FnMut(Option<&[u8]>)> = Box::new(|data: Option<&[u8]>| {
            if let Some(data) = data {
                let escaped: String = data
                    .iter()
                    .flat_map(|b| std::ascii::escape_default(*b))
                    .map(|c| c as char)
                    .collect();
                info!("[STDOUT DATA] {}", escaped);
            } else {
                info!("[STDOUT EOF]");
            }
        });
        let stderr_callback: Box<dyn FnMut(Option<&[u8]>)> = Box::new(|data: Option<&[u8]>| {
            if let Some(data) = data {
                let escaped: String = data
                    .iter()
                    .flat_map(|b| std::ascii::escape_default(*b))
                    .map(|c| c as char)
                    .collect();
                info!("[STDERR DATA] {}", escaped);
            } else {
                info!("[STDERR EOF]");
            }
        });

        command
            .arg("/tmp")
            .current_dir(&Path::from("/tmp"))
            .stdout(Stdio::piped(stdout_callback))
            .stderr(Stdio::piped(stderr_callback));
        let child = command.spawn().expect("Spawn failure");
        info!("Spawn result was: {:?}", child.wait().await);
    }
}
