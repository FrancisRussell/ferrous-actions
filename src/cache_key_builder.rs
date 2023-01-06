use crate::actions::cache::Entry as CacheEntry;
use crate::hasher::Blake3 as Blake3Hasher;
use crate::{node, safe_encoding};
use std::collections::BTreeMap;

const CACHE_ENTRY_VERSION: &str = "7";

pub struct CacheKeyBuilder {
    name: String,
    hasher: Blake3Hasher,
    key_attributes: BTreeMap<&'static str, String>,
    attributes: BTreeMap<&'static str, String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, strum::Display, strum::IntoStaticStr, Ord, PartialEq, PartialOrd)]
pub enum KeyAttribute {
    #[strum(serialize = "id")]
    Id,

    #[strum(serialize = "job")]
    Job,

    #[strum(serialize = "nonce")]
    Matrix,

    #[strum(serialize = "platform")]
    Platform,

    #[strum(serialize = "workflow")]
    Workflow,
}

#[derive(Clone, Copy, Debug, Eq, Hash, strum::Display, strum::IntoStaticStr, Ord, PartialEq, PartialOrd)]
pub enum Attribute {
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

    #[strum(serialize = "version")]
    Version,
}

impl CacheKeyBuilder {
    fn empty(name: &str) -> CacheKeyBuilder {
        let mut result = CacheKeyBuilder {
            name: name.into(),
            hasher: Blake3Hasher::default(),
            key_attributes: BTreeMap::new(),
            attributes: BTreeMap::new(),
        };
        result.add_key_data(CACHE_ENTRY_VERSION);
        result
    }

    pub fn new(name: &str) -> CacheKeyBuilder {
        use crate::nonce;

        let mut result = Self::empty(name);
        result.set_key_attribute(KeyAttribute::Platform, node::os::platform());
        let date = chrono::Local::now();
        result.set_attribute(Attribute::Timestamp, date.to_string());
        let nonce = nonce::build(8);
        let nonce = safe_encoding::encode(nonce);
        result.set_attribute(Attribute::Nonce, nonce);
        result
    }

    pub fn add_key_data<T: std::hash::Hash + ?Sized>(&mut self, data: &T) {
        data.hash(&mut self.hasher);
        let id: [u8; 32] = self.hasher.inner().finalize().into();
        let id = &id[..8];
        let id = safe_encoding::encode(id);
        self.key_attributes.insert(KeyAttribute::Id.into(), id);
    }

    pub fn set_key_attribute(&mut self, key: KeyAttribute, value: String) {
        assert_ne!(key, KeyAttribute::Id, "ID attribute cannot be set directly");
        self.key_attributes.insert(key.into(), value);
    }

    pub fn set_attribute(&mut self, name: Attribute, value: String) {
        self.attributes.insert(name.into(), value);
    }

    fn restore_key_to_save_key(&self, restore_key: &str) -> String {
        use itertools::Itertools as _;

        let mut save_key = restore_key.to_string();
        if !self.attributes.is_empty() {
            save_key += " (";
            save_key += &self.attributes.iter().map(|(a, v)| format!("{}={}", a, v)).join("; ");
            save_key += ")";
        }
        save_key.replace(',', ";")
    }

    fn current_restore_key(&self) -> String {
        use itertools::Itertools as _;

        let mut key_mappings = String::from("(");
        if !self.key_attributes.is_empty() {
            key_mappings += &self
                .key_attributes
                .iter()
                .map(|(a, v)| format!("{}={}", a, v))
                .join("; ");
        }
        key_mappings += ")";
        let restore_key = format!("Ferrous Actions: {} - key={}", self.name, key_mappings);
        restore_key.replace(',', ";")
    }

    pub fn into_entry(self) -> CacheEntry {
        let restore_key = self.current_restore_key();
        let save_key = self.restore_key_to_save_key(&restore_key);
        let mut result = CacheEntry::new(save_key.as_str());
        result.restore_key(restore_key);
        result
    }
}
