use std::collections::BTreeMap;

use crate::firestore::value::FirestoreValue;

#[derive(Clone, Debug, PartialEq)]
pub struct MapValue {
    fields: BTreeMap<String, FirestoreValue>,
}

impl MapValue {
    pub fn new(fields: BTreeMap<String, FirestoreValue>) -> Self {
        Self { fields }
    }

    pub fn fields(&self) -> &BTreeMap<String, FirestoreValue> {
        &self.fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_map_entries() {
        let mut map = BTreeMap::new();
        map.insert("foo".to_string(), FirestoreValue::from_integer(1));
        let value = MapValue::new(map.clone());
        assert_eq!(value.fields().get("foo"), map.get("foo"));
    }
}
