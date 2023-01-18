use crate::node;
use crate::node::path::Path;
use wasm_bindgen::JsValue;

#[derive(Debug)]
pub struct ScopedCwd {
    original: Path,
}

impl ScopedCwd {
    pub fn new(new_cwd: &Path) -> Result<ScopedCwd, JsValue> {
        let cwd = node::process::cwd();
        node::process::chdir(new_cwd)?;
        Ok(ScopedCwd { original: cwd })
    }
}

impl Drop for ScopedCwd {
    fn drop(&mut self) {
        node::process::chdir(&self.original)
            .unwrap_or_else(|e| panic!("Unable to chdir back to original folder: {:?}", e));
    }
}
