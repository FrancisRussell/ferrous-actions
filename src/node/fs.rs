use crate::node::path::Path;
use chrono::{DateTime, NaiveDateTime, Utc};
use js_sys::{BigInt, JsString, Object, Uint8Array};
use std::collections::VecDeque;
use wasm_bindgen::{JsCast, JsError, JsValue};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FileType {
    inner: FileTypeEnum,
}

impl FileType {
    pub fn is_file(self) -> bool {
        self.inner == FileTypeEnum::File
    }

    pub fn is_dir(self) -> bool {
        self.inner == FileTypeEnum::Dir
    }

    pub fn is_symlink(self) -> bool {
        self.inner == FileTypeEnum::Symlink
    }

    pub fn is_fifo(self) -> bool {
        self.inner == FileTypeEnum::Fifo
    }

    pub fn is_socket(self) -> bool {
        self.inner == FileTypeEnum::Socket
    }

    pub fn is_block_device(self) -> bool {
        self.inner == FileTypeEnum::BlockDev
    }

    pub fn is_char_device(self) -> bool {
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
    Unknown,
}

fn determine_file_type(file_type: &ffi::FileType) -> FileTypeEnum {
    if file_type.is_block_device() {
        FileTypeEnum::BlockDev
    } else if file_type.is_character_device() {
        FileTypeEnum::CharDev
    } else if file_type.is_socket() {
        FileTypeEnum::Socket
    } else if file_type.is_fifo() {
        FileTypeEnum::Fifo
    } else if file_type.is_symbolic_link() {
        FileTypeEnum::Symlink
    } else if file_type.is_directory() {
        FileTypeEnum::Dir
    } else if file_type.is_file() {
        FileTypeEnum::File
    } else {
        FileTypeEnum::Unknown
    }
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
        FileType {
            inner: determine_file_type(&self.inner),
        }
    }
}

impl Iterator for ReadDir {
    type Item = DirEntry;

    fn next(&mut self) -> Option<DirEntry> {
        let parent = self.path.clone();
        self.entries.pop_front().map(|inner| DirEntry { parent, inner })
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

pub async fn write_file<P: Into<JsString>>(path: P, data: &[u8]) -> Result<(), JsValue> {
    let path: JsString = path.into();
    ffi::write_file(&path, data).await?;
    Ok(())
}

pub async fn read_dir<P: Into<JsString>>(path: P) -> Result<ReadDir, JsValue> {
    let path: JsString = path.into();
    let options = js_sys::Map::new();
    options.set(&"withFileTypes".into(), &true.into());
    options.set(&"encoding".into(), &"utf8".into());
    let options = Object::from_entries(&options).expect("Failed to convert options map to object");
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
    let options = js_sys::Map::new();
    options.set(&"recursive".into(), &true.into());
    let options = Object::from_entries(&options).expect("Failed to convert options map to object");
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

#[derive(Debug)]
pub struct Metadata {
    inner: ffi::Stats,
}

impl Metadata {
    pub fn uid(&self) -> u64 {
        self.inner.uid().try_into().expect("UID too large")
    }

    pub fn gid(&self) -> u64 {
        self.inner.gid().try_into().expect("GID too large")
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> u64 {
        self.inner.size().try_into().expect("File size too large")
    }

    pub fn mode(&self) -> u64 {
        self.inner.mode().try_into().expect("File mode too large")
    }

    fn utc_ns_to_time(ns: BigInt) -> DateTime<Utc> {
        const NS_IN_S: i128 = 1000 * 1000 * 1000;
        let ns = i128::try_from(ns).expect("Timestamp out of range");
        let (secs, subsec_nanos) = {
            let mut seconds = ns / NS_IN_S;
            let mut nanoseconds = ns % NS_IN_S;
            if nanoseconds < 0 {
                seconds -= 1;
                nanoseconds += NS_IN_S;
            }
            (seconds, nanoseconds)
        };
        let secs: i64 = secs.try_into().expect("Seconds out of range");
        let subsec_nanos: u32 = subsec_nanos.try_into().expect("Nanoseconds out of range");
        let naive = NaiveDateTime::from_timestamp_opt(secs, subsec_nanos).expect("File time out of bounds");
        DateTime::from_utc(naive, Utc)
    }

    pub fn accessed(&self) -> DateTime<Utc> {
        let ns = self.inner.access_time_ns();
        Self::utc_ns_to_time(ns)
    }

    pub fn modified(&self) -> DateTime<Utc> {
        let ns = self.inner.modification_time_ns();
        Self::utc_ns_to_time(ns)
    }

    pub fn created(&self) -> DateTime<Utc> {
        let ns = self.inner.created_time_ns();
        Self::utc_ns_to_time(ns)
    }

    pub fn file_type(&self) -> FileType {
        FileType {
            inner: determine_file_type(&self.inner),
        }
    }

    pub fn is_directory(&self) -> bool {
        self.inner.is_directory()
    }
}

pub async fn symlink_metadata<P: Into<JsString>>(path: P) -> Result<Metadata, JsValue> {
    let path = path.into();
    let options = js_sys::Map::new();
    options.set(&"bigint".into(), &true.into());
    let options = Object::from_entries(&options).expect("Failed to convert options map to object");
    let stats = ffi::lstat(&path, Some(options)).await.map(Into::<ffi::Stats>::into)?;
    Ok(Metadata { inner: stats })
}

fn timestamp_to_seconds(timestamp: &DateTime<Utc>) -> f64 {
    // utimes takes timestamps in seconds - this was fun to debug
    const NS_IN_S: f64 = 1e9;
    #[allow(clippy::cast_precision_loss)]
    let whole = timestamp.timestamp() as f64;
    let fractional = f64::from(timestamp.timestamp_subsec_nanos()) / NS_IN_S;
    whole + fractional
}

pub async fn lutimes<P: Into<JsString>>(
    path: P,
    a_time: &DateTime<Utc>,
    m_time: &DateTime<Utc>,
) -> Result<(), JsValue> {
    use js_sys::Number;

    let path = path.into();
    let a_time: Number = timestamp_to_seconds(a_time).into();
    let m_time: Number = timestamp_to_seconds(m_time).into();
    ffi::lutimes(&path, a_time.as_ref(), m_time.as_ref()).await?;
    Ok(())
}

pub mod ffi {
    use js_sys::{BigInt, JsString, Object};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsValue;

    #[wasm_bindgen]
    extern "C" {
        #[derive(Debug)]
        pub type FileType;

        #[wasm_bindgen(method, js_name = "isDirectory")]
        pub fn is_directory(this: &FileType) -> bool;

        #[wasm_bindgen(method, js_name = "isFile")]
        pub fn is_file(this: &FileType) -> bool;

        #[wasm_bindgen(method, js_name = "isBlockDevice")]
        pub fn is_block_device(this: &FileType) -> bool;

        #[wasm_bindgen(method, js_name = "isCharacterDevice")]
        pub fn is_character_device(this: &FileType) -> bool;

        #[wasm_bindgen(method, js_name = "isFIFO")]
        pub fn is_fifo(this: &FileType) -> bool;

        #[wasm_bindgen(method, js_name = "isSocket")]
        pub fn is_socket(this: &FileType) -> bool;

        #[wasm_bindgen(method, js_name = "isSymbolicLink")]
        pub fn is_symbolic_link(this: &FileType) -> bool;
    }

    #[wasm_bindgen(module = "fs")]
    extern "C" {
        #[derive(Debug)]
        #[wasm_bindgen(js_name = "DirEnt", extends = FileType)]
        pub type DirEnt;

        #[wasm_bindgen(method, getter, js_name = "name")]
        pub fn get_name(this: &DirEnt) -> JsString;

        #[derive(Debug)]
        #[wasm_bindgen(js_name = "Stats", extends = FileType)]
        pub type Stats;

        #[wasm_bindgen(method, getter)]
        pub fn size(this: &Stats) -> BigInt;

        #[wasm_bindgen(method, getter, js_name = "atimeNs")]
        pub fn access_time_ns(this: &Stats) -> BigInt;

        #[wasm_bindgen(method, getter, js_name = "mtimeNs")]
        pub fn modification_time_ns(this: &Stats) -> BigInt;

        #[wasm_bindgen(method, getter, js_name = "birthtimeNs")]
        pub fn created_time_ns(this: &Stats) -> BigInt;

        #[wasm_bindgen(method, getter)]
        pub fn uid(this: &Stats) -> BigInt;

        #[wasm_bindgen(method, getter)]
        pub fn gid(this: &Stats) -> BigInt;

        #[wasm_bindgen(method, getter)]
        pub fn mode(this: &Stats) -> BigInt;
    }

    #[wasm_bindgen(module = "fs/promises")]
    extern "C" {
        #[wasm_bindgen(catch)]
        pub async fn chmod(path: &JsString, mode: u16) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch, js_name = "readFile")]
        pub async fn read_file(path: &JsString) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch, js_name = "writeFile")]
        pub async fn write_file(path: &JsString, data: &[u8]) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch, js_name = "readdir")]
        pub async fn read_dir(path: &JsString, options: Option<Object>) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch)]
        pub async fn mkdir(path: &JsString, options: Option<Object>) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch)]
        pub async fn rename(old: &JsString, new: &JsString) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch)]
        pub async fn rmdir(path: &JsString, options: Option<Object>) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch)]
        pub async fn access(path: &JsString, mode: Option<u32>) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch)]
        pub async fn lstat(path: &JsString, options: Option<Object>) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(catch)]
        pub async fn lutimes(path: &JsString, atime: &JsValue, mtime: &JsValue) -> Result<JsValue, JsValue>;
    }
}
