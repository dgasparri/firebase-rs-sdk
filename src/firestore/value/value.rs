use std::collections::BTreeMap;

use crate::firestore::model::{GeoPoint, Timestamp};
use crate::firestore::value::{ArrayValue, BytesValue, MapValue};

#[derive(Clone, Debug, PartialEq)]
pub struct FirestoreValue {
    kind: ValueKind,
}

/// Sentinel transforms supported during writes.
///
/// Mirrors the modular JS sentinel implementations from the Firebase JS SDK
/// (see `packages/firestore/src/lite-api/field_value_impl.ts`).
#[derive(Clone, Debug, PartialEq)]
pub enum SentinelValue {
    ServerTimestamp,
    ArrayUnion(Vec<FirestoreValue>),
    ArrayRemove(Vec<FirestoreValue>),
    NumericIncrement(Box<FirestoreValue>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ValueKind {
    Null,
    Boolean(bool),
    Integer(i64),
    Double(f64),
    Timestamp(Timestamp),
    String(String),
    Bytes(BytesValue),
    Reference(String),
    GeoPoint(GeoPoint),
    Array(ArrayValue),
    Map(MapValue),
    Sentinel(SentinelValue),
}

impl FirestoreValue {
    pub fn null() -> Self {
        Self {
            kind: ValueKind::Null,
        }
    }

    pub fn from_bool(value: bool) -> Self {
        Self {
            kind: ValueKind::Boolean(value),
        }
    }

    pub fn from_integer(value: i64) -> Self {
        Self {
            kind: ValueKind::Integer(value),
        }
    }

    pub fn from_double(value: f64) -> Self {
        Self {
            kind: ValueKind::Double(value),
        }
    }

    pub fn from_timestamp(value: Timestamp) -> Self {
        Self {
            kind: ValueKind::Timestamp(value),
        }
    }

    pub fn from_string(value: impl Into<String>) -> Self {
        Self {
            kind: ValueKind::String(value.into()),
        }
    }

    pub fn from_bytes(value: BytesValue) -> Self {
        Self {
            kind: ValueKind::Bytes(value),
        }
    }

    pub fn from_reference(path: impl Into<String>) -> Self {
        Self {
            kind: ValueKind::Reference(path.into()),
        }
    }

    pub fn from_geo_point(value: GeoPoint) -> Self {
        Self {
            kind: ValueKind::GeoPoint(value),
        }
    }

    pub fn from_array(values: Vec<FirestoreValue>) -> Self {
        Self {
            kind: ValueKind::Array(ArrayValue::new(values)),
        }
    }

    pub fn from_map(map: BTreeMap<String, FirestoreValue>) -> Self {
        Self {
            kind: ValueKind::Map(MapValue::new(map)),
        }
    }

    /// Returns a sentinel that instructs Firestore to populate the field with the server timestamp.
    ///
    /// TypeScript reference: `serverTimestamp()` in
    /// `packages/firestore/src/lite-api/field_value_impl.ts`.
    pub fn server_timestamp() -> Self {
        Self {
            kind: ValueKind::Sentinel(SentinelValue::ServerTimestamp),
        }
    }

    /// Returns a sentinel that unions the provided elements with an existing array field.
    ///
    /// TypeScript reference: `arrayUnion(...)` in
    /// `packages/firestore/src/lite-api/field_value_impl.ts`.
    pub fn array_union(elements: Vec<FirestoreValue>) -> Self {
        Self {
            kind: ValueKind::Sentinel(SentinelValue::ArrayUnion(elements)),
        }
    }

    /// Returns a sentinel that removes the provided elements from an existing array field.
    ///
    /// TypeScript reference: `arrayRemove(...)` in
    /// `packages/firestore/src/lite-api/field_value_impl.ts`.
    pub fn array_remove(elements: Vec<FirestoreValue>) -> Self {
        Self {
            kind: ValueKind::Sentinel(SentinelValue::ArrayRemove(elements)),
        }
    }

    /// Returns a sentinel that increments the targeted numeric field by `operand`.
    ///
    /// TypeScript reference: `increment(...)` in
    /// `packages/firestore/src/lite-api/field_value_impl.ts`.
    pub fn numeric_increment(operand: FirestoreValue) -> Self {
        Self {
            kind: ValueKind::Sentinel(SentinelValue::NumericIncrement(Box::new(operand))),
        }
    }

    pub fn kind(&self) -> &ValueKind {
        &self.kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_basic_values() {
        let v = FirestoreValue::from_string("hello");
        match v.kind() {
            ValueKind::String(value) => assert_eq!(value, "hello"),
            _ => panic!("unexpected kind"),
        }
    }
}
