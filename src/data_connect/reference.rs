use std::sync::Arc;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::data_connect::api::DataConnectService;
use crate::data_connect::config::DataConnectOptions;

/// Indicates where a result originated from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DataSource {
    Cache,
    Server,
}

impl DataSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            DataSource::Cache => "CACHE",
            DataSource::Server => "SERVER",
        }
    }
}

/// Internal discriminant for refs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationType {
    Query,
    Mutation,
}

/// Common state for query & mutation references.
#[derive(Clone, Debug)]
pub struct OperationRef {
    pub(crate) service: Arc<DataConnectService>,
    pub(crate) name: Arc<str>,
    pub(crate) variables: Value,
    #[allow(dead_code)]
    pub(crate) op_type: OperationType,
}

impl OperationRef {
    pub fn operation_name(&self) -> &str {
        &self.name
    }

    pub fn variables(&self) -> &Value {
        &self.variables
    }

    pub fn service(&self) -> &Arc<DataConnectService> {
        &self.service
    }
}

/// Strongly typed reference to a query operation.
#[derive(Clone, Debug)]
pub struct QueryRef(pub(crate) OperationRef);

impl QueryRef {
    pub fn operation_name(&self) -> &str {
        self.0.operation_name()
    }

    pub fn variables(&self) -> &Value {
        self.0.variables()
    }

    pub fn service(&self) -> &Arc<DataConnectService> {
        self.0.service()
    }
}

/// Strongly typed reference to a mutation operation.
#[derive(Clone, Debug)]
pub struct MutationRef(pub(crate) OperationRef);

impl MutationRef {
    pub fn operation_name(&self) -> &str {
        self.0.operation_name()
    }

    pub fn variables(&self) -> &Value {
        self.0.variables()
    }

    pub fn service(&self) -> &Arc<DataConnectService> {
        self.0.service()
    }
}

/// Minimal payload cached by the query manager.
#[derive(Clone, Debug)]
pub struct OpResult {
    pub data: Value,
    pub source: DataSource,
    pub fetch_time: SystemTime,
}

/// Result returned from `execute_query`.
#[derive(Clone, Debug)]
pub struct QueryResult {
    pub data: Value,
    pub source: DataSource,
    pub fetch_time: SystemTime,
    pub query_ref: QueryRef,
}

impl QueryResult {
    pub fn to_serialized(&self) -> SerializedQuerySnapshot {
        SerializedQuerySnapshot {
            data: self.data.clone(),
            source: self.source,
            fetch_time: system_time_to_string(self.fetch_time),
            ref_info: RefInfo {
                name: self.query_ref.operation_name().to_string(),
                variables: self.query_ref.variables().clone(),
                connector_config: self.query_ref.service().options().clone(),
            },
        }
    }
}

/// Result returned from `execute_mutation`.
#[derive(Clone, Debug)]
pub struct MutationResult {
    pub data: Value,
    pub source: DataSource,
    pub fetch_time: SystemTime,
    pub mutation_ref: MutationRef,
}

/// Serializable reference snapshot (mirrors JS SDK `SerializedRef`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedQuerySnapshot {
    pub data: Value,
    pub source: DataSource,
    pub fetch_time: String,
    pub ref_info: RefInfo,
}

/// Serialized reference metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefInfo {
    pub name: String,
    pub variables: Value,
    pub connector_config: DataConnectOptions,
}

pub(crate) fn system_time_to_string(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339()
}

pub(crate) fn string_to_system_time(value: &str) -> Option<SystemTime> {
    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_rfc2822(value))
        .ok()
        .map(SystemTime::from)
}

pub(crate) fn encode_query_key(name: &str, variables: &Value) -> String {
    let payload = serde_json::json!({
        "name": name,
        "variables": variables,
    });
    serde_json::to_string(&payload).expect("query key serialization")
}
