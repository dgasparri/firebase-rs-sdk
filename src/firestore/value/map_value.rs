use std::collections::BTreeMap;

use crate::firestore::model::FieldPath;
use crate::firestore::value::{FirestoreValue, ValueKind};

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

    /// Retrieves a value referenced by the provided field path if it exists.
    ///
    /// This powers higher-level helpers such as
    /// [`crate::firestore::api::snapshot::DocumentSnapshot::get`].
    pub fn get(&self, field_path: &FieldPath) -> Option<&FirestoreValue> {
        get_from_segments(self.fields(), field_path.segments())
    }
}

fn get_from_segments<'a>(
    fields: &'a BTreeMap<String, FirestoreValue>,
    segments: &[String],
) -> Option<&'a FirestoreValue> {
    let (first, rest) = segments.split_first()?;
    let value = fields.get(first)?;
    if rest.is_empty() {
        Some(value)
    } else if let ValueKind::Map(child) = value.kind() {
        get_from_segments(child.fields(), rest)
    } else {
        None
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

    #[test]
    fn get_returns_nested_value() {
        let mut inner = BTreeMap::new();
        inner.insert("bar".to_string(), FirestoreValue::from_string("baz"));
        let mut map = BTreeMap::new();
        map.insert("foo".to_string(), FirestoreValue::from_map(inner));
        let value = MapValue::new(map);
        let path = FieldPath::from_dot_separated("foo.bar").unwrap();
        let result = value.get(&path).unwrap();
        match result.kind() {
            ValueKind::String(s) => assert_eq!(s, "baz"),
            _ => panic!("expected string"),
        }
    }
}
