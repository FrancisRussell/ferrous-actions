use std::collections::BTreeMap;
use crate::actions::cache::CacheEntry;

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
        let primary_key = format!("GitHub Rust Actions: {} - id={}", self.name, id);
        let primary_key = primary_key.replace(',', ";");
        let mut result = CacheEntry::new(primary_key.as_str());
        if !self.attributes.is_empty() {
            let mut secondary_key = primary_key;
            secondary_key += " (";
            let mut first = true;
            for (attribute, value) in self.attributes {
                if first {
                    first = false;
                } else {
                    secondary_key += "; ";
                }
                secondary_key += &format!("{}={}", attribute, value);
            }
            secondary_key += " )";
            let secondary_key = secondary_key.replace(',', ";");
            result.restore_key(secondary_key);
        }
        result
    }
}
