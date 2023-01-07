use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

pub fn serialize_btree_map<K, V, S>(map: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    K: Serialize,
    V: Serialize,
{
    let map_as_vec: Vec<(&K, &V)> = map.iter().collect();
    map_as_vec.serialize(serializer)
}

pub fn deserialize_btree_map<'a, K, V, D>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'a>,
    K: Deserialize<'a> + Ord,
    V: Deserialize<'a>,
{
    let deserialized: Vec<(K, V)> = Vec::deserialize(deserializer)?;
    Ok(deserialized.into_iter().collect())
}
