use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectMetadata {
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub full_path: Option<String>,
    #[serde(default)]
    pub generation: Option<String>,
    #[serde(default)]
    pub metageneration: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub time_created: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub cache_control: Option<String>,
    #[serde(default)]
    pub content_disposition: Option<String>,
    #[serde(default)]
    pub content_language: Option<String>,
    #[serde(default)]
    pub content_encoding: Option<String>,
    #[serde(default)]
    pub custom_metadata: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub download_tokens: Option<String>,
    #[serde(skip)]
    pub raw: Value,
}

impl ObjectMetadata {
    pub fn from_value(value: Value) -> Self {
        let mut metadata: ObjectMetadata =
            serde_json::from_value(value.clone()).unwrap_or_else(|_| ObjectMetadata::default());
        metadata.raw = value;
        metadata
    }

    pub fn raw(&self) -> &Value {
        &self.raw
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetMetadataRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_disposition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_encoding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_metadata: Option<BTreeMap<String, String>>,
}
