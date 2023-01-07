use derivative::Derivative;
use serde::de::SeqAccess;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::marker::PhantomData;

pub fn serialize_btree_map<K, V, S>(map: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    K: Serialize,
    V: Serialize,
{
    let mut seq = serializer.serialize_seq(Some(map.len()))?;
    for element in map {
        seq.serialize_element(&element)?;
    }
    seq.end()
}

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
#[derive(Debug)]
struct Visitor<K, V> {
    _key: PhantomData<K>,
    _value: PhantomData<V>,
}

impl<'de, K, V> serde::de::Visitor<'de> for Visitor<K, V>
where
    K: Ord + Deserialize<'de>,
    V: Deserialize<'de>,
{
    type Value = BTreeMap<K, V>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(formatter, "an ordered sequence of key-value pairs")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut map = BTreeMap::new();
        while let Some((k, v)) = seq.next_element()? {
            map.insert(k, v);
        }
        Ok(map)
    }
}

pub fn deserialize_btree_map<'a, K, V, D>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'a>,
    K: Deserialize<'a> + Ord,
    V: Deserialize<'a>,
{
    let visitor = Visitor::default();
    deserializer.deserialize_seq(visitor)
}
