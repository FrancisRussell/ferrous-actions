pub mod os {
    use super::path;

    pub fn platform() -> String {
        ffi::platform().into()
    }

    pub fn machine() -> String {
        ffi::machine().into()
    }

    pub fn arch() -> String {
        ffi::arch().into()
    }

    pub fn homedir() -> path::Path {
        path::Path::from(ffi::homedir())
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(module = "os")]
        extern "C" {
            pub fn arch() -> JsString;
            pub fn homedir() -> JsString;
            pub fn machine() -> JsString;
            pub fn platform() -> JsString;
        }
    }
}

pub mod fs {
    use crate::node::path::Path;
    use js_sys::{JsString, Uint8Array};
    use std::collections::VecDeque;
    use wasm_bindgen::{JsCast, JsError, JsValue};

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub struct FileType {
        inner: FileTypeEnum,
    }

    impl FileType {
        pub fn is_file(&self) -> bool {
            self.inner == FileTypeEnum::File
        }

        pub fn is_dir(&self) -> bool {
            self.inner == FileTypeEnum::Dir
        }

        pub fn is_symlink(&self) -> bool {
            self.inner == FileTypeEnum::Symlink
        }

        pub fn is_fifo(&self) -> bool {
            self.inner == FileTypeEnum::Fifo
        }

        pub fn is_socket(&self) -> bool {
            self.inner == FileTypeEnum::Socket
        }

        pub fn is_block_device(&self) -> bool {
            self.inner == FileTypeEnum::BlockDev
        }

        pub fn is_char_device(&self) -> bool {
            self.inner == FileTypeEnum::CharDev
        }
    }

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    enum FileTypeEnum {
        File,
        Dir,
        Symlink,
        BlockDev,
        CharDev,
        Fifo,
        Socket,
    }

    #[derive(Debug)]
    pub struct ReadDir {
        path: Path,
        entries: VecDeque<ffi::DirEnt>,
    }

    #[derive(Debug)]
    pub struct DirEntry {
        parent: Path,
        inner: ffi::DirEnt,
    }

    impl DirEntry {
        pub fn file_name(&self) -> String {
            self.inner.get_name().into()
        }

        pub fn path(&self) -> Path {
            let mut result = self.parent.clone();
            result.push(self.inner.get_name());
            result
        }

        pub fn file_type(&self) -> FileType {
            let inner = if self.inner.is_block_device() {
                FileTypeEnum::BlockDev
            } else if self.inner.is_character_device() {
                FileTypeEnum::CharDev
            } else if self.inner.is_socket() {
                FileTypeEnum::Socket
            } else if self.inner.is_fifo() {
                FileTypeEnum::Fifo
            } else if self.inner.is_symbolic_link() {
                FileTypeEnum::Symlink
            } else if self.inner.is_directory() {
                FileTypeEnum::Dir
            } else if self.inner.is_file() {
                FileTypeEnum::File
            } else {
                panic!("Unhandled directory entry type")
            };
            FileType { inner }
        }
    }

    impl Iterator for ReadDir {
        type Item = DirEntry;

        fn next(&mut self) -> Option<DirEntry> {
            let parent = self.path.clone();
            self.entries
                .pop_front()
                .map(|inner| DirEntry { parent, inner })
        }
    }

    pub async fn chmod<P: Into<JsString>>(path: P, mode: u16) -> Result<(), JsValue> {
        let path: JsString = path.into();
        ffi::chmod(&path, mode).await.map(|_| ())
    }

    pub async fn read_file<P: Into<JsString>>(path: P) -> Result<Vec<u8>, JsValue> {
        let path: JsString = path.into();
        let buffer = ffi::read_file(&path).await?;
        let buffer = buffer
            .dyn_ref::<Uint8Array>()
            .ok_or_else(|| JsError::new("readFile didn't return an array"))?;
        let length = buffer.length();
        let mut result = vec![0u8; length as usize];
        buffer.copy_to(&mut result);
        Ok(result)
    }

    pub async fn read_dir<P: Into<JsString>>(path: P) -> Result<ReadDir, JsValue> {
        use js_sys::Object;

        let path: JsString = path.into();
        let options = js_sys::Map::new();
        options.set(&"withFileTypes".into(), &true.into());
        options.set(&"encoding".into(), &"utf8".into());
        let options =
            Object::from_entries(&options).expect("Failed to convert options map to object");
        let entries = ffi::read_dir(&path, Some(options)).await?;
        let entries: VecDeque<_> = entries
            .dyn_into::<js_sys::Array>()
            .map_err(|_| JsError::new("read_dir didn't return an array"))?
            .iter()
            .map(Into::<ffi::DirEnt>::into)
            .collect();
        let path = Path::from(path);
        let entries = ReadDir { path, entries };
        Ok(entries)
    }

    pub async fn create_dir_all<P: Into<JsString>>(path: P) -> Result<(), JsValue> {
        use js_sys::Object;

        let options = js_sys::Map::new();
        options.set(&"recursive".into(), &true.into());
        let options =
            Object::from_entries(&options).expect("Failed to convert options map to object");
        let path: JsString = path.into();
        ffi::mkdir(&path, Some(options)).await?;
        Ok(())
    }

    pub async fn create_dir<P: Into<JsString>>(path: P) -> Result<(), JsValue> {
        let path: JsString = path.into();
        ffi::mkdir(&path, None).await?;
        Ok(())
    }

    pub async fn remove_dir<P: Into<JsString>>(path: P) -> Result<(), JsValue> {
        let path: JsString = path.into();
        ffi::rmdir(&path, None).await?;
        Ok(())
    }

    pub async fn rename<P: Into<JsString>>(from: P, to: P) -> Result<(), JsValue> {
        let from: JsString = from.into();
        let to: JsString = to.into();
        ffi::rename(&from, &to).await?;
        Ok(())
    }

    pub mod ffi {
        use js_sys::JsString;
        use js_sys::Object;
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsValue;

        #[wasm_bindgen(module = "fs")]
        extern "C" {
            #[wasm_bindgen(js_name = "Dirent")]
            #[derive(Debug)]
            pub type DirEnt;

            #[wasm_bindgen(method, js_class = "Dirent", js_name = "isDirectory")]
            pub fn is_directory(this: &DirEnt) -> bool;

            #[wasm_bindgen(method, js_class = "Dirent", js_name = "isFile")]
            pub fn is_file(this: &DirEnt) -> bool;

            #[wasm_bindgen(method, js_class = "Dirent", js_name = "isBlockDevice")]
            pub fn is_block_device(this: &DirEnt) -> bool;

            #[wasm_bindgen(method, js_class = "Dirent", js_name = "isCharacterDevice")]
            pub fn is_character_device(this: &DirEnt) -> bool;

            #[wasm_bindgen(method, js_class = "Dirent", js_name = "isFIFO")]
            pub fn is_fifo(this: &DirEnt) -> bool;

            #[wasm_bindgen(method, js_class = "Dirent", js_name = "isSocket")]
            pub fn is_socket(this: &DirEnt) -> bool;

            #[wasm_bindgen(method, js_class = "Dirent", js_name = "isSymbolicLink")]
            pub fn is_symbolic_link(this: &DirEnt) -> bool;

            #[wasm_bindgen(method, getter, js_name = "name")]
            pub fn get_name(this: &DirEnt) -> JsString;
        }

        #[wasm_bindgen(module = "fs/promises")]
        extern "C" {
            #[wasm_bindgen(catch)]
            pub async fn chmod(path: &JsString, mode: u16) -> Result<JsValue, JsValue>;

            #[wasm_bindgen(catch, js_name = "readFile")]
            pub async fn read_file(path: &JsString) -> Result<JsValue, JsValue>;

            #[wasm_bindgen(catch, js_name = "readdir")]
            pub async fn read_dir(
                path: &JsString,
                options: Option<Object>,
            ) -> Result<JsValue, JsValue>;

            #[wasm_bindgen(catch)]
            pub async fn mkdir(
                path: &JsString,
                options: Option<Object>,
            ) -> Result<JsValue, JsValue>;

            #[wasm_bindgen(catch)]
            pub async fn rename(old: &JsString, new: &JsString) -> Result<JsValue, JsValue>;

            #[wasm_bindgen(catch)]
            pub async fn rmdir(
                path: &JsString,
                options: Option<Object>,
            ) -> Result<JsValue, JsValue>;
        }
    }
}

pub mod path {
    use js_sys::JsString;

    pub struct Path {
        inner: JsString,
    }

    impl std::fmt::Display for Path {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
            let string = String::from(&self.inner);
            formatter.write_str(string.as_str())
        }
    }

    impl Path {
        pub fn push<P: Into<Path>>(&mut self, path: P) {
            let path = path.into();
            let joined = ffi::resolve(vec![self.inner.to_string(), path.inner.to_string()]);
            self.inner = joined;
        }

        pub fn to_js_string(&self) -> JsString {
            self.inner.to_string()
        }

        pub fn parent(&self) -> Path {
            let parent = ffi::dirname(&self.inner);
            Path { inner: parent }
        }
    }

    impl std::fmt::Debug for Path {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
            write!(formatter, "{}", self)
        }
    }

    impl Clone for Path {
        fn clone(&self) -> Path {
            Path {
                inner: self.inner.to_string(),
            }
        }
    }

    impl From<JsString> for Path {
        fn from(path: JsString) -> Path {
            let path = ffi::normalize(&path);
            Path { inner: path }
        }
    }

    impl From<&str> for Path {
        fn from(path: &str) -> Path {
            let path: JsString = path.into();
            let path = ffi::normalize(&path);
            Path { inner: path }
        }
    }

    impl From<&Path> for JsString {
        fn from(path: &Path) -> JsString {
            path.inner.to_string()
        }
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(module = "path")]
        extern "C" {
            pub fn normalize(path: &JsString) -> JsString;
            #[wasm_bindgen(variadic)]
            pub fn join(paths: Vec<JsString>) -> JsString;
            #[wasm_bindgen(variadic)]
            pub fn resolve(paths: Vec<JsString>) -> JsString;
            #[wasm_bindgen]
            pub fn dirname(path: &JsString) -> JsString;
        }
    }
}

pub mod process {
    use super::path;

    pub fn cwd() -> path::Path {
        path::Path::from(ffi::cwd())
    }

    pub mod ffi {
        use js_sys::JsString;
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(module = "process")]
        extern "C" {
            pub fn cwd() -> JsString;
        }
    }
}
