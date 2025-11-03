use std::collections::BTreeMap;
use std::str::FromStr;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde_json::{json, Value as JsonValue};

use crate::firestore::api::operations::{FieldTransform, TransformOperation};
use crate::firestore::error::{invalid_argument, FirestoreResult};
use crate::firestore::model::{DatabaseId, DocumentKey, FieldPath, GeoPoint, Timestamp};
use crate::firestore::value::{BytesValue, FirestoreValue, MapValue, ValueKind};

#[derive(Clone, Debug)]
pub struct JsonProtoSerializer {
    database_id: DatabaseId,
}

impl JsonProtoSerializer {
    pub fn new(database_id: DatabaseId) -> Self {
        Self { database_id }
    }

    pub fn database_id(&self) -> &DatabaseId {
        &self.database_id
    }

    pub fn database_name(&self) -> String {
        format!(
            "projects/{}/databases/{}",
            self.database_id.project_id(),
            self.database_id.database()
        )
    }

    pub fn document_name(&self, key: &DocumentKey) -> String {
        format!(
            "{}/documents/{}",
            self.database_name(),
            key.path().canonical_string()
        )
    }

    pub fn encode_document_fields(&self, map: &MapValue) -> JsonValue {
        json!({
            "fields": encode_map_fields(map)
        })
    }

    pub fn encode_commit_body(&self, key: &DocumentKey, map: &MapValue) -> JsonValue {
        json!({
            "writes": [ self.encode_set_write(key, map, &[]) ]
        })
    }

    pub fn encode_merge_body(
        &self,
        key: &DocumentKey,
        map: &MapValue,
        field_paths: &[FieldPath],
        transforms: &[FieldTransform],
    ) -> JsonValue {
        json!({
            "writes": [ self.encode_merge_write(key, map, field_paths, transforms) ]
        })
    }

    pub fn encode_update_body(
        &self,
        key: &DocumentKey,
        map: &MapValue,
        field_paths: &[FieldPath],
        transforms: &[FieldTransform],
    ) -> JsonValue {
        json!({
            "writes": [ self.encode_update_write(key, map, field_paths, transforms) ]
        })
    }

    pub fn encode_delete_body(&self, key: &DocumentKey) -> JsonValue {
        json!({
            "writes": [ self.encode_delete_write(key) ]
        })
    }

    fn build_update_write_map(
        &self,
        key: &DocumentKey,
        map: &MapValue,
        transforms: &[FieldTransform],
    ) -> serde_json::Map<String, JsonValue> {
        let mut write = serde_json::Map::new();
        write.insert(
            "update".to_string(),
            json!({
                "name": self.document_name(key),
                "fields": encode_map_fields(map)
            }),
        );
        if let Some(encoded) = self.encode_field_transforms(transforms) {
            write.insert("updateTransforms".to_string(), JsonValue::Array(encoded));
        }
        write
    }

    fn encode_field_transforms(&self, transforms: &[FieldTransform]) -> Option<Vec<JsonValue>> {
        if transforms.is_empty() {
            return None;
        }

        let mut encoded = Vec::with_capacity(transforms.len());
        for transform in transforms {
            let field_path = transform.field_path().canonical_string();
            let json = match transform.operation() {
                TransformOperation::ServerTimestamp => json!({
                    "fieldPath": field_path,
                    "setToServerValue": "REQUEST_TIME"
                }),
                TransformOperation::ArrayUnion(elements) => json!({
                    "fieldPath": field_path,
                    "appendMissingElements": {
                        "values": elements
                            .iter()
                            .map(|value| self.encode_value(value))
                            .collect::<Vec<_>>()
                    }
                }),
                TransformOperation::ArrayRemove(elements) => json!({
                    "fieldPath": field_path,
                    "removeAllFromArray": {
                        "values": elements
                            .iter()
                            .map(|value| self.encode_value(value))
                            .collect::<Vec<_>>()
                    }
                }),
                TransformOperation::NumericIncrement(operand) => json!({
                    "fieldPath": field_path,
                    "increment": self.encode_value(operand)
                }),
            };
            encoded.push(json);
        }

        Some(encoded)
    }

    pub fn encode_set_write(
        &self,
        key: &DocumentKey,
        map: &MapValue,
        transforms: &[FieldTransform],
    ) -> JsonValue {
        JsonValue::Object(self.build_update_write_map(key, map, transforms))
    }

    pub fn encode_merge_write(
        &self,
        key: &DocumentKey,
        map: &MapValue,
        field_paths: &[FieldPath],
        transforms: &[FieldTransform],
    ) -> JsonValue {
        let mut write = self.build_update_write_map(key, map, transforms);
        let mask: Vec<String> = field_paths
            .iter()
            .map(FieldPath::canonical_string)
            .collect();
        write.insert("updateMask".to_string(), json!({ "fieldPaths": mask }));
        JsonValue::Object(write)
    }

    pub fn encode_update_write(
        &self,
        key: &DocumentKey,
        map: &MapValue,
        field_paths: &[FieldPath],
        transforms: &[FieldTransform],
    ) -> JsonValue {
        let mut write = self.build_update_write_map(key, map, transforms);
        if !field_paths.is_empty() {
            let mask: Vec<String> = field_paths
                .iter()
                .map(FieldPath::canonical_string)
                .collect();
            write.insert("updateMask".to_string(), json!({ "fieldPaths": mask }));
        }
        write.insert("currentDocument".to_string(), json!({ "exists": true }));
        JsonValue::Object(write)
    }

    pub fn encode_delete_write(&self, key: &DocumentKey) -> JsonValue {
        json!({
            "delete": self.document_name(key)
        })
    }

    pub fn decode_document_fields(&self, value: &JsonValue) -> FirestoreResult<Option<MapValue>> {
        if value.get("fields").is_some() {
            decode_map_value(value).map(Some)
        } else {
            // Document exists but has no user fields.
            Ok(Some(MapValue::new(BTreeMap::new())))
        }
    }

    pub fn decode_map_value(&self, value: &JsonValue) -> FirestoreResult<MapValue> {
        decode_map_value(value)
    }

    pub fn encode_value(&self, value: &FirestoreValue) -> JsonValue {
        encode_value(value)
    }
}

fn encode_map_fields(map: &MapValue) -> JsonValue {
    let mut fields = serde_json::Map::new();
    for (key, value) in map.fields() {
        fields.insert(key.clone(), encode_value(value));
    }
    JsonValue::Object(fields)
}

fn encode_value(value: &FirestoreValue) -> JsonValue {
    match value.kind() {
        ValueKind::Null => json!({ "nullValue": JsonValue::Null }),
        ValueKind::Boolean(boolean) => json!({ "booleanValue": boolean }),
        ValueKind::Integer(integer) => json!({ "integerValue": integer.to_string() }),
        ValueKind::Double(double) => json!({ "doubleValue": double }),
        ValueKind::Timestamp(timestamp) => json!({ "timestampValue": encode_timestamp(timestamp) }),
        ValueKind::String(string) => json!({ "stringValue": string }),
        ValueKind::Bytes(bytes) => {
            json!({ "bytesValue": BASE64_STANDARD.encode(bytes.as_slice()) })
        }
        ValueKind::Reference(reference) => json!({ "referenceValue": reference }),
        ValueKind::GeoPoint(point) => json!({
            "geoPointValue": {
                "latitude": point.latitude(),
                "longitude": point.longitude(),
            }
        }),
        ValueKind::Array(array) => {
            let values = array.values().iter().map(encode_value).collect::<Vec<_>>();
            json!({ "arrayValue": { "values": values } })
        }
        ValueKind::Map(map) => json!({
            "mapValue": {
                "fields": encode_map_fields(map)
            }
        }),
        ValueKind::Sentinel(_) => panic!("sentinel values must be handled as field transforms"),
    }
}

fn decode_map_value(value: &JsonValue) -> FirestoreResult<MapValue> {
    let map = value
        .as_object()
        .ok_or_else(|| invalid_argument("Expected object for map value"))?;
    let fields_object = match map.get("fields") {
        Some(fields_value) => fields_value
            .as_object()
            .ok_or_else(|| invalid_argument("Expected 'fields' to be an object"))?,
        None => return Ok(MapValue::new(BTreeMap::new())),
    };

    let mut fields = BTreeMap::new();
    for (key, value) in fields_object {
        fields.insert(key.clone(), decode_value(value)?);
    }
    Ok(MapValue::new(fields))
}

fn decode_value(value: &JsonValue) -> FirestoreResult<FirestoreValue> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_argument("Expected Firestore value object"))?;
    if let Some(null_value) = object.get("nullValue") {
        if null_value.is_null() {
            return Ok(FirestoreValue::null());
        }
    }
    if let Some(bool_value) = object.get("booleanValue") {
        let value = bool_value
            .as_bool()
            .ok_or_else(|| invalid_argument("booleanValue must be bool"))?;
        return Ok(FirestoreValue::from_bool(value));
    }
    if let Some(integer_value) = object.get("integerValue") {
        let parsed = match integer_value {
            JsonValue::String(value) => i64::from_str(value)
                .map_err(|err| invalid_argument(format!("Invalid integerValue: {err}")))?,
            JsonValue::Number(number) => number
                .as_i64()
                .ok_or_else(|| invalid_argument("Integer out of range"))?,
            _ => return Err(invalid_argument("integerValue must be a string or number")),
        };
        return Ok(FirestoreValue::from_integer(parsed));
    }
    if let Some(double_value) = object.get("doubleValue") {
        let parsed = match double_value {
            JsonValue::Number(number) => number
                .as_f64()
                .ok_or_else(|| invalid_argument("Invalid doubleValue"))?,
            JsonValue::String(value) => value
                .parse::<f64>()
                .map_err(|err| invalid_argument(format!("Invalid doubleValue: {err}")))?,
            _ => return Err(invalid_argument("doubleValue must be a number or string")),
        };
        return Ok(FirestoreValue::from_double(parsed));
    }
    if let Some(timestamp_value) = object.get("timestampValue") {
        let timestamp_str = timestamp_value
            .as_str()
            .ok_or_else(|| invalid_argument("timestampValue must be string"))?;
        return Ok(FirestoreValue::from_timestamp(parse_timestamp(
            timestamp_str,
        )?));
    }
    if let Some(string_value) = object.get("stringValue") {
        let str_value = string_value
            .as_str()
            .ok_or_else(|| invalid_argument("stringValue must be string"))?;
        return Ok(FirestoreValue::from_string(str_value));
    }
    if let Some(bytes_value) = object.get("bytesValue") {
        let str_value = bytes_value
            .as_str()
            .ok_or_else(|| invalid_argument("bytesValue must be base64 string"))?;
        let decoded = BASE64_STANDARD
            .decode(str_value)
            .map_err(|err| invalid_argument(format!("Invalid bytesValue: {err}")))?;
        return Ok(FirestoreValue::from_bytes(BytesValue::from(decoded)));
    }
    if let Some(reference_value) = object.get("referenceValue") {
        let str_value = reference_value
            .as_str()
            .ok_or_else(|| invalid_argument("referenceValue must be string"))?;
        return Ok(FirestoreValue::from_reference(str_value));
    }
    if let Some(geo_point) = object.get("geoPointValue") {
        let latitude = geo_point
            .get("latitude")
            .and_then(|value| value.as_f64())
            .ok_or_else(|| invalid_argument("geoPointValue.latitude must be f64"))?;
        let longitude = geo_point
            .get("longitude")
            .and_then(|value| value.as_f64())
            .ok_or_else(|| invalid_argument("geoPointValue.longitude must be f64"))?;
        return Ok(FirestoreValue::from_geo_point(GeoPoint::new(
            latitude, longitude,
        )?));
    }
    if let Some(array_value) = object.get("arrayValue") {
        let decoded = if let Some(values) = array_value.get("values") {
            match values.as_array() {
                Some(entries) => entries
                    .iter()
                    .map(decode_value)
                    .collect::<FirestoreResult<Vec<_>>>()?,
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        return Ok(FirestoreValue::from_array(decoded));
    }
    if let Some(map_value) = object.get("mapValue") {
        let map = decode_map_value(map_value)?;
        return Ok(FirestoreValue::from_map(map.fields().clone()));
    }

    Err(invalid_argument("Unknown Firestore value type"))
}

fn encode_timestamp(timestamp: &Timestamp) -> String {
    Utc.timestamp_opt(timestamp.seconds, timestamp.nanos as u32)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("zero timestamp"))
        .to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn parse_timestamp(value: &str) -> FirestoreResult<Timestamp> {
    let datetime = DateTime::parse_from_rfc3339(value)
        .map_err(|err| invalid_argument(format!("Invalid timestamp: {err}")))?;
    let datetime_utc = datetime.with_timezone(&Utc);
    Ok(Timestamp::new(
        datetime_utc.timestamp(),
        datetime_utc.timestamp_subsec_nanos() as i32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let mut map = BTreeMap::new();
        map.insert("name".to_string(), FirestoreValue::from_string("Ada"));
        map.insert("age".to_string(), FirestoreValue::from_integer(42));
        map.insert(
            "nested".to_string(),
            FirestoreValue::from_map({
                let mut inner = BTreeMap::new();
                inner.insert("flag".to_string(), FirestoreValue::from_bool(true));
                inner
            }),
        );
        let map = MapValue::new(map);
        let serializer = JsonProtoSerializer::new(DatabaseId::default("project"));
        let encoded = serializer.encode_document_fields(&map);
        let decoded = serializer.decode_document_fields(&encoded).unwrap();
        assert!(decoded.is_some());
        let decoded_map = decoded.unwrap();
        assert_eq!(
            decoded_map.fields().get("name"),
            Some(&FirestoreValue::from_string("Ada"))
        );
        assert_eq!(
            decoded_map.fields().get("age"),
            Some(&FirestoreValue::from_integer(42))
        );
    }
}
