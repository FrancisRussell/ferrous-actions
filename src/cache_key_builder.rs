use crate::actions::cache::Entry as CacheEntry;
use crate::hasher::Blake3 as Blake3Hasher;
use crate::{node, safe_encoding};
use std::collections::BTreeMap;

const CACHE_ENTRY_VERSION: &str = "15";

pub struct CacheKeyBuilder {
    name: String,
    hasher: Blake3Hasher,
    attributes: BTreeMap<&'static str, (String, bool)>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, strum::Display, strum::IntoStaticStr, Ord, PartialEq, PartialOrd)]
pub enum Attribute {
    #[strum(serialize = "job")]
    Job,

    #[strum(serialize = "matrix")]
    Matrix,

    #[strum(serialize = "platform")]
    Platform,

    #[strum(serialize = "workflow")]
    Workflow,

    #[strum(serialize = "args_truncated")]
    ArgsTruncated,

    #[strum(serialize = "nonce")]
    Nonce,

    #[strum(serialize = "num_entries")]
    NumEntries,

    #[strum(serialize = "path")]
    Path,

    #[strum(serialize = "date")]
    Timestamp,

    #[strum(serialize = "target")]
    Target,

    #[strum(serialize = "toolchain_version")]
    ToolchainVersion,

    #[strum(serialize = "entries_hash")]
    EntriesHash,
}

impl CacheKeyBuilder {
    fn empty(name: &str) -> CacheKeyBuilder {
        let mut result = CacheKeyBuilder {
            name: name.into(),
            hasher: Blake3Hasher::default(),
            attributes: BTreeMap::new(),
        };
        result.add_key_data(CACHE_ENTRY_VERSION);
        result
    }

    pub fn new(name: &str) -> CacheKeyBuilder {
        use crate::nonce;

        let mut result = Self::empty(name);
        result.set_key_attribute(Attribute::Platform, node::os::platform());
        let date = chrono::Local::now();
        result.set_attribute(Attribute::Timestamp, date.to_string());
        let nonce = nonce::build(8);
        let nonce = safe_encoding::encode(nonce);
        result.set_attribute(Attribute::Nonce, nonce);
        result
    }

    pub fn add_key_data<T: std::hash::Hash + ?Sized>(&mut self, data: &T) {
        data.hash(&mut self.hasher);
    }

    pub fn set_key_attribute(&mut self, key: Attribute, value: String) {
        self.attributes.insert(key.into(), (value, true));
    }

    pub fn set_attribute(&mut self, name: Attribute, value: String) {
        self.attributes.insert(name.into(), (value, false));
    }

    fn restore_key_to_save_key(restore_key: &str, attributes: &BTreeMap<&str, (String, bool)>) -> String {
        use itertools::Itertools as _;
        use std::fmt::Write as _;

        let mut save_key = restore_key.to_string();
        if !attributes.is_empty() {
            save_key += ", attributes={";
            write!(
                save_key,
                "{}",
                attributes.iter().map(|(a, v)| format!("{}={}", a, v.0)).format("; ")
            )
            .expect("Unable to format restore key");
            save_key += "}";
        }
        save_key.replace(',', ";")
    }

    fn build_restore_key(name: &str, mut hasher: Blake3Hasher, attributes: &BTreeMap<&str, (String, bool)>) -> String {
        use std::hash::Hash as _;

        let id = {
            attributes
                .iter()
                .filter_map(|(k, v)| v.1.then_some((k, &v.0)))
                .for_each(|v| v.hash(&mut hasher));
            let id: [u8; 32] = hasher.inner().finalize().into();
            let id = &id[..8];
            safe_encoding::encode(id)
        };

        let restore_key = format!("Ferrous Actions: {} - id={}", name, id);
        restore_key.replace(',', ";")
    }

    pub fn into_entry(self) -> CacheEntry {
        let restore_key = Self::build_restore_key(&self.name, self.hasher, &self.attributes);
        let save_key = Self::restore_key_to_save_key(&restore_key, &self.attributes);
        let mut result = CacheEntry::new(save_key.as_str());
        result.restore_key(restore_key);
        // Since we have the "platform" attribute, turning this on makes no difference
        // unless the user overrides it
        result.permit_sharing_with_windows(true);
        result
    }
}
