use super::path::Path;
use crate::error;
use futures::channel::oneshot;
use futures::future::{FutureExt as _, Shared};
use futures::AsyncRead;
use js_sys::{JsString, Object};
use parking_lot::Mutex;
use std::borrow::Cow;
use std::pin::Pin;
use std::sync::Arc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast as _, JsValue};

enum StdioEnum {
    Ignore,
    Inherit,
    Piped,
}

impl StdioEnum {
    fn nodejs_name(&self) -> Cow<'static, str> {
        match self {
            StdioEnum::Inherit => "inherit",
            StdioEnum::Ignore => "ignore",
            StdioEnum::Piped => "pipe",
        }
        .into()
    }
}

/// Where output of a standard stream can be redirected
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

    /// Constructs a `Stdio` which causes output to be sent or received by the
    /// specified callback.
    pub fn piped() -> Stdio {
        Stdio {
            inner: StdioEnum::Piped,
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
    _subprocess: Object,
    _error_callback: Closure<dyn FnMut(JsValue)>,
    _close_callback: Closure<dyn FnMut(JsValue, JsValue)>,
    _mutable: Arc<Mutex<ChildStateMutable>>,
    #[allow(clippy::type_complexity)]
    completion: Shared<Pin<Box<dyn futures::Future<Output = Result<ExitStatus, JsValue>>>>>,
}

pub struct Child {
    state: Arc<ChildState>,
    stdout_handle: Option<ChildOutputStream>,
    stderr_handle: Option<ChildOutputStream>,
}

impl Child {
    pub fn wait(&self) -> impl futures::Future<Output = Result<ExitStatus, JsValue>> {
        self.state.completion.clone()
    }

    pub fn take_stdout(&mut self) -> Option<ChildOutputStream> {
        self.stdout_handle.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildOutputStream> {
        self.stderr_handle.take()
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

        let mut stdout_handle = None;
        let mut stderr_handle = None;
        for (name, mode, handle) in [
            ("stdout", &mut self.stdout.inner, &mut stdout_handle),
            ("stderr", &mut self.stderr.inner, &mut stderr_handle),
        ] {
            if matches!(mode, StdioEnum::Piped) {
                *handle = {
                    let stream_js = js_sys::Reflect::get(&subprocess, &name.into())?.dyn_into::<js_sys::Object>()?;
                    let handle = ChildOutputStream::try_from(stream_js)?;
                    Some(handle)
                };
            }
        }

        let child_state = ChildState {
            _subprocess: subprocess,
            _error_callback: error_callback,
            _close_callback: close_callback,
            _mutable: child_state_mutable,
            completion: completion.shared(),
        };
        Ok(Child {
            state: Arc::new(child_state),
            stdout_handle,
            stderr_handle,
        })
    }

    /// Sets the current working directory of the command
    pub fn current_dir(&mut self, path: &Path) -> &mut Command {
        self.cwd = Some(path.clone());
        self
    }

    /// Sets where standard error should be directed
    pub fn stderr(&mut self, redirect: Stdio) -> &mut Command {
        self.stderr = redirect;
        self
    }

    /// Sets where standard output should be directed
    pub fn stdout(&mut self, redirect: Stdio) -> &mut Command {
        self.stdout = redirect;
        self
    }
}

#[derive(Default)]
struct ChildOutputStreamStateShared {
    waker: Option<futures::task::Waker>,
    ended: bool,
    error: Option<JsValue>,
}

pub struct ChildOutputStream {
    stream: Object,
    readable_closure: Closure<dyn Fn()>,
    end_closure: Closure<dyn Fn()>,
    error_closure: Closure<dyn Fn(JsValue)>,
    shared: Arc<Mutex<ChildOutputStreamStateShared>>,
    read_fn: js_sys::Function,
    off_fn: js_sys::Function,
    destroy_fn: js_sys::Function,
}

impl TryFrom<Object> for ChildOutputStream {
    type Error = JsValue;

    fn try_from(stream: Object) -> Result<Self, Self::Error> {
        // Pause the stream
        let pause_fn = js_sys::Reflect::get(&stream, &"pause".into())?.dyn_into::<js_sys::Function>()?;
        pause_fn.call0(&stream)?;

        let on_fn = js_sys::Reflect::get(&stream, &"on".into())?.dyn_into::<js_sys::Function>()?;
        let shared: Arc<Mutex<ChildOutputStreamStateShared>> = Arc::default();
        let shared_readable = shared.clone();
        let readable_closure: Closure<dyn Fn()> = Closure::new(move || {
            let waker = shared_readable.lock().waker.take();
            if let Some(waker) = waker {
                waker.wake();
            }
        });
        on_fn.call2(&stream, &"readable".into(), readable_closure.as_ref())?;

        let shared_end = shared.clone();
        let end_closure: Closure<dyn Fn()> = Closure::new(move || {
            let mut state = shared_end.lock();
            state.ended = true;
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
        });
        on_fn.call2(&stream, &"end".into(), end_closure.as_ref())?;

        let shared_error = shared.clone();
        let error_closure: Closure<dyn Fn(JsValue)> = Closure::new(move |e: JsValue| {
            let mut state = shared_error.lock();
            state.error.get_or_insert(e);
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
        });
        on_fn.call2(&stream, &"error".into(), error_closure.as_ref())?;

        let read_fn = js_sys::Reflect::get(&stream, &"read".into())?.dyn_into::<js_sys::Function>()?;
        let off_fn = js_sys::Reflect::get(&stream, &"off".into())?.dyn_into::<js_sys::Function>()?;
        let destroy_fn = js_sys::Reflect::get(&stream, &"destroy".into())?.dyn_into::<js_sys::Function>()?;
        Ok(ChildOutputStream {
            stream,
            readable_closure,
            end_closure,
            error_closure,
            shared,
            read_fn,
            off_fn,
            destroy_fn,
        })
    }
}

impl AsyncRead for ChildOutputStream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut futures::task::Context<'_>,
        buf: &mut [u8],
    ) -> futures::task::Poll<Result<usize, std::io::Error>> {
        use futures::task::Poll;

        // A read limit of 1GiB is specified by Node
        let max_read = std::cmp::min(buf.len(), 64 * 1024 * 1024);
        let data = match self.read_fn.call1(&self.stream, &JsValue::from(max_read)) {
            Ok(value) => value,
            Err(e) => {
                self.shared.lock().error.get_or_insert(e);
                JsValue::NULL
            }
        };

        if data.is_null_or_undefined() {
            let mut guard = self.shared.lock();
            if let Some(error) = &guard.error {
                Poll::Ready(Err(std::io::Error::other(format!("{:?}", error))))
            } else if guard.ended {
                Poll::Ready(Ok(0))
            } else {
                guard.waker = Some(context.waker().clone());
                Poll::Pending
            }
        } else {
            let data: js_sys::Uint8Array = data.into();
            let num_bytes = usize::try_from(data.length()).expect("Array too large");
            data.copy_to(&mut buf[..num_bytes]);
            Poll::Ready(Ok(num_bytes))
        }
    }
}

impl Drop for ChildOutputStream {
    fn drop(&mut self) {
        // Unregister any callbacks
        for (name, closure) in [
            ("readable", self.readable_closure.as_ref()),
            ("end", self.end_closure.as_ref()),
            ("error", self.error_closure.as_ref()),
        ] {
            if let Err(e) = self.off_fn.call2(&self.stream, &name.into(), closure) {
                error!(
                    "Failed to unregister {} closure from child output stream: {:?}",
                    name, e
                );
            }
        }

        // Close the stream
        if let Err(e) = self.destroy_fn.call0(&self.stream) {
            error!("Failed to call destroy on child output stream: {:?}", e);
        }
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
        use futures::AsyncReadExt as _;

        let mut command = Command::from(&Path::from("ls"));
        command
            .arg("/tmp")
            .current_dir(&Path::from("/tmp"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("Spawn failure");

        // Read streams
        let stdout_data = if let Some(mut stream) = child.take_stdout() {
            let mut buffer = Vec::new();
            stream
                .read_to_end(&mut buffer)
                .await
                .expect("Failed to read stdout to end.");
            Some(buffer)
        } else {
            None
        };
        let stderr_data = if let Some(mut stream) = child.take_stderr() {
            let mut buffer = Vec::new();
            stream
                .read_to_end(&mut buffer)
                .await
                .expect("Failed to read stderr to end.");
            Some(buffer)
        } else {
            None
        };

        // Wait for process completion
        info!("Spawn result was: {:?}", child.wait().await);

        // Print data
        if let Some(data) = stdout_data {
            let escaped: String = data
                .iter()
                .flat_map(|b| std::ascii::escape_default(*b))
                .map(|c| c as char)
                .collect();
            info!("[STDOUT DATA] {}", escaped);
        }
        if let Some(data) = stderr_data {
            let escaped: String = data
                .iter()
                .flat_map(|b| std::ascii::escape_default(*b))
                .map(|c| c as char)
                .collect();
            info!("[STDERR DATA] {}", escaped);
        }
    }
}
