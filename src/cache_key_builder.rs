use crate::actions::cache::CacheEntry;
use std::collections::BTreeMap;

pub struct CacheKeyBuilder {
    name: String,
    hasher: blake3::Hasher,
    attributes: BTreeMap<String, String>,
}

impl CacheKeyBuilder {
    pub fn new(name: &str) -> CacheKeyBuilder {
        CacheKeyBuilder {
            name: name.into(),
            hasher: blake3::Hasher::new(),
            attributes: BTreeMap::new(),
        }
    }

    pub fn add_id_bytes(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    pub fn set_attribute(&mut self, name: &str, value: &str) {
        self.attributes.insert(name.into(), value.into());
    }

    pub fn set_attribute_nonce(&mut self, name: &str) {
        use crate::nonce::build_nonce;
        let nonce = build_nonce(8);
        let nonce = base64::encode_config(nonce, base64::URL_SAFE);
        self.set_attribute(name, &nonce);
    }

    pub fn into_entry(self) -> CacheEntry {
        let id: [u8; 32] = self.hasher.finalize().into();
        let id = &id[..8];
        let id = base64::encode_config(id, base64::URL_SAFE);
        let restore_key = format!("GitHub Rust Actions: {} - id={}", self.name, id);
        let restore_key = restore_key.replace(',', ";");
        let mut save_key = restore_key.clone();
        if !self.attributes.is_empty() {
            save_key += " (";
            let mut first = true;
            for (attribute, value) in self.attributes {
                if first {
                    first = false;
                } else {
                    save_key += "; ";
                }
                save_key += &format!("{}={}", attribute, value);
            }
            save_key += ")";
        }
        let save_key = save_key.replace(',', ";");
        let mut result = CacheEntry::new(save_key.as_str());
        result.restore_key(restore_key);
        result
    }
}
