use std::collections::BTreeMap;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde::Deserialize;
use serde_json::{
    //json, 
    Value as JsonValue};

use crate::firestore::error::{
    deadline_exceeded, internal_error, invalid_argument, not_found, permission_denied,
    resource_exhausted, unauthenticated, unavailable, FirestoreError, FirestoreResult,
};
use crate::firestore::model::{DocumentKey, Timestamp};
use crate::firestore::remote::serializer::JsonProtoSerializer;
use crate::firestore::value::MapValue;

#[derive(Debug, Clone)]
pub enum WatchChange {
    TargetChange(WatchTargetChange),
    DocumentChange(DocumentChange),
    DocumentDelete(DocumentDelete),
    DocumentRemove(DocumentRemove),
    ExistenceFilter(ExistenceFilterChange),
}

#[derive(Debug, Clone)]
pub struct WatchTargetChange {
    pub state: TargetChangeState,
    pub target_ids: Vec<i32>,
    pub resume_token: Option<Vec<u8>>,
    pub read_time: Option<Timestamp>,
    pub cause: Option<FirestoreError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetChangeState {
    NoChange,
    Add,
    Remove,
    Current,
    Reset,
}

#[derive(Debug, Clone)]
pub struct DocumentChange {
    pub updated_target_ids: Vec<i32>,
    pub removed_target_ids: Vec<i32>,
    pub document: Option<WatchDocument>,
}

#[derive(Debug, Clone)]
pub struct WatchDocument {
    pub key: DocumentKey,
    pub fields: MapValue,
    pub update_time: Option<Timestamp>,
    pub create_time: Option<Timestamp>,
}

#[derive(Debug, Clone)]
pub struct DocumentDelete {
    pub key: DocumentKey,
    pub read_time: Option<Timestamp>,
    pub removed_target_ids: Vec<i32>,
}

#[derive(Debug, Clone)]
pub struct DocumentRemove {
    pub key: DocumentKey,
    pub read_time: Option<Timestamp>,
    pub removed_target_ids: Vec<i32>,
}

#[derive(Debug, Clone)]
pub struct ExistenceFilterChange {
    pub target_id: i32,
    pub count: i32,
}

#[derive(Debug, Deserialize)]
struct StatusCause {
    code: i32,
    #[serde(default)]
    message: Option<String>,
}

pub fn decode_watch_change(
    serializer: &JsonProtoSerializer,
    value: &JsonValue,
) -> FirestoreResult<Option<WatchChange>> {
    if let Some(target_change) = value.get("targetChange") {
        return decode_target_change(serializer, target_change).map(Some);
    }

    if let Some(document_change) = value.get("documentChange") {
        return decode_document_change(serializer, document_change).map(Some);
    }

    if let Some(document_delete) = value.get("documentDelete") {
        return decode_document_delete(serializer, document_delete).map(Some);
    }

    if let Some(document_remove) = value.get("documentRemove") {
        return decode_document_remove(serializer, document_remove).map(Some);
    }

    if let Some(filter) = value.get("filter") {
        return decode_filter_change(filter).map(Some);
    }

    Ok(None)
}

fn decode_target_change(
    serializer: &JsonProtoSerializer,
    value: &JsonValue,
) -> FirestoreResult<WatchChange> {
    let target_ids = value
        .get("targetIds")
        .and_then(JsonValue::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_i64().map(|id| id as i32))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let resume_token = value
        .get("resumeToken")
        .and_then(JsonValue::as_str)
        .and_then(|token| BASE64_STANDARD.decode(token).ok());

    let read_time = value
        .get("readTime")
        .and_then(JsonValue::as_str)
        .map(|timestamp| serializer.decode_timestamp_string(timestamp))
        .transpose()?;

    let state = value
        .get("targetChangeType")
        .and_then(JsonValue::as_str)
        .map(target_state_from_str)
        .unwrap_or(TargetChangeState::NoChange);

    let cause = value
        .get("cause")
        .map(|cause| serde_json::from_value::<StatusCause>(cause.clone()))
        .transpose()
        .map_err(|err| internal_error(format!("Failed to decode watch cause: {err}")))?
        .map(|cause| map_grpc_status(cause.code, cause.message));

    Ok(WatchChange::TargetChange(WatchTargetChange {
        state,
        target_ids,
        resume_token,
        read_time,
        cause,
    }))
}

fn decode_document_change(
    serializer: &JsonProtoSerializer,
    value: &JsonValue,
) -> FirestoreResult<WatchChange> {
    let updated_target_ids = numeric_array(value.get("targetIds"));
    let removed_target_ids = numeric_array(value.get("removedTargetIds"));
    let document = value
        .get("document")
        .map(|doc| decode_watch_document(serializer, doc))
        .transpose()?;

    Ok(WatchChange::DocumentChange(DocumentChange {
        updated_target_ids,
        removed_target_ids,
        document,
    }))
}

fn decode_document_delete(
    serializer: &JsonProtoSerializer,
    value: &JsonValue,
) -> FirestoreResult<WatchChange> {
    let name = value
        .get("document")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| invalid_argument("documentDelete missing document"))?;
    let key = serializer.document_key_from_name(name)?;
    let read_time = value
        .get("readTime")
        .and_then(JsonValue::as_str)
        .map(|timestamp| serializer.decode_timestamp_string(timestamp))
        .transpose()?;
    let removed_target_ids = numeric_array(value.get("removedTargetIds"));

    Ok(WatchChange::DocumentDelete(DocumentDelete {
        key,
        read_time,
        removed_target_ids,
    }))
}

fn decode_document_remove(
    serializer: &JsonProtoSerializer,
    value: &JsonValue,
) -> FirestoreResult<WatchChange> {
    let name = value
        .get("document")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| invalid_argument("documentRemove missing document"))?;
    let key = serializer.document_key_from_name(name)?;
    let read_time = value
        .get("readTime")
        .and_then(JsonValue::as_str)
        .map(|timestamp| serializer.decode_timestamp_string(timestamp))
        .transpose()?;
    let removed_target_ids = numeric_array(value.get("removedTargetIds"));

    Ok(WatchChange::DocumentRemove(DocumentRemove {
        key,
        read_time,
        removed_target_ids,
    }))
}

fn decode_filter_change(value: &JsonValue) -> FirestoreResult<WatchChange> {
    let target_id = value
        .get("targetId")
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| invalid_argument("filter missing targetId"))? as i32;
    let count = value
        .get("count")
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| invalid_argument("filter missing count"))? as i32;
    Ok(WatchChange::ExistenceFilter(ExistenceFilterChange {
        target_id,
        count,
    }))
}

fn decode_watch_document(
    serializer: &JsonProtoSerializer,
    document: &JsonValue,
) -> FirestoreResult<WatchDocument> {
    let name = document
        .get("name")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| invalid_argument("Watch document missing name"))?;
    let key = serializer.document_key_from_name(name)?;
    let fields = serializer
        .decode_document_fields(document)?
        .unwrap_or_else(|| MapValue::new(BTreeMap::new()));

    let update_time = document
        .get("updateTime")
        .and_then(JsonValue::as_str)
        .map(|timestamp| serializer.decode_timestamp_string(timestamp))
        .transpose()?;
    let create_time = document
        .get("createTime")
        .and_then(JsonValue::as_str)
        .map(|timestamp| serializer.decode_timestamp_string(timestamp))
        .transpose()?;

    Ok(WatchDocument {
        key,
        fields,
        update_time,
        create_time,
    })
}

fn numeric_array(value: Option<&JsonValue>) -> Vec<i32> {
    value
        .and_then(JsonValue::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_i64().map(|value| value as i32))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn target_state_from_str(value: &str) -> TargetChangeState {
    match value {
        "NO_CHANGE" => TargetChangeState::NoChange,
        "ADD" => TargetChangeState::Add,
        "REMOVE" => TargetChangeState::Remove,
        "CURRENT" => TargetChangeState::Current,
        "RESET" => TargetChangeState::Reset,
        _ => TargetChangeState::NoChange,
    }
}

fn map_grpc_status(code: i32, message: Option<String>) -> FirestoreError {
    let message = message.unwrap_or_else(|| "watch stream error".to_string());
    match code {
        3 => invalid_argument(message),
        4 => deadline_exceeded(message),
        5 => not_found(message),
        7 => permission_denied(message),
        8 => resource_exhausted(message),
        13 => internal_error(message),
        14 => unavailable(message),
        16 => unauthenticated(message),
        _ => internal_error(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::model::DatabaseId;
    use serde_json::json;

    fn serializer() -> JsonProtoSerializer {
        JsonProtoSerializer::new(DatabaseId::new("project", "(default)"))
    }

    #[test]
    fn decodes_target_change() {
        let change = json!({
            "targetChange": {
                "targetIds": [1, 2],
                "resumeToken": BASE64_STANDARD.encode([1u8, 2, 3]),
                "targetChangeType": "CURRENT"
            }
        });

        let decoded = decode_watch_change(&serializer(), &change)
            .unwrap()
            .unwrap();
        match decoded {
            WatchChange::TargetChange(change) => {
                assert_eq!(change.target_ids, vec![1, 2]);
                assert_eq!(change.resume_token.as_deref(), Some(&[1, 2, 3][..]));
                assert_eq!(change.state, TargetChangeState::Current);
            }
            other => panic!("unexpected change: {other:?}"),
        }
    }
}
