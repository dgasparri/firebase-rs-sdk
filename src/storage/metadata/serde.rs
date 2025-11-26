use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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
    #[serde(
        default,
        deserialize_with = "deserialize_option_u64_from_string",
        serialize_with = "serialize_option_u64_as_string"
    )]
    pub size: Option<u64>,
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
    pub md5_hash: Option<String>,
    #[serde(default)]
    pub crc32c: Option<String>,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default, rename = "metadata")]
    pub custom_metadata: Option<BTreeMap<String, String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_download_tokens",
        serialize_with = "serialize_download_tokens"
    )]
    pub download_tokens: Option<Vec<String>>,
    #[serde(skip)]
    pub raw: Value,
}

impl ObjectMetadata {
    pub fn from_value(value: Value) -> Self {
        let mut metadata: ObjectMetadata = serde_json::from_value(value.clone()).unwrap_or_default();
        metadata.raw = value;
        metadata
    }

    pub fn raw(&self) -> &Value {
        &self.raw
    }

    pub fn size_bytes(&self) -> Option<u64> {
        self.size
    }

    pub fn download_tokens(&self) -> Option<&[String]> {
        self.download_tokens.as_deref()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none", rename = "metadata")]
    pub custom_metadata: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crc32c: Option<String>,
}

pub type SettableMetadata = SetMetadataRequest;
pub type UploadMetadata = SetMetadataRequest;

impl SetMetadataRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cache_control(mut self, value: impl Into<String>) -> Self {
        self.cache_control = Some(value.into());
        self
    }

    pub fn with_content_disposition(mut self, value: impl Into<String>) -> Self {
        self.content_disposition = Some(value.into());
        self
    }

    pub fn with_content_encoding(mut self, value: impl Into<String>) -> Self {
        self.content_encoding = Some(value.into());
        self
    }

    pub fn with_content_language(mut self, value: impl Into<String>) -> Self {
        self.content_language = Some(value.into());
        self
    }

    pub fn with_content_type(mut self, value: impl Into<String>) -> Self {
        self.content_type = Some(value.into());
        self
    }

    pub fn with_md5_hash(mut self, value: impl Into<String>) -> Self {
        self.md5_hash = Some(value.into());
        self
    }

    pub fn with_crc32c(mut self, value: impl Into<String>) -> Self {
        self.crc32c = Some(value.into());
        self
    }

    pub fn insert_custom_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.custom_metadata
            .get_or_insert_with(BTreeMap::new)
            .insert(key.into(), value.into());
    }

    pub fn custom_metadata(&self) -> Option<&BTreeMap<String, String>> {
        self.custom_metadata.as_ref()
    }
}

fn deserialize_option_u64_from_string<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let option: Option<Value> = Option::deserialize(deserializer)?;
    match option {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => Ok(number.as_u64()),
        Some(Value::String(s)) => {
            if s.trim().is_empty() {
                Ok(None)
            } else {
                s.parse::<u64>()
                    .map(Some)
                    .map_err(|err| D::Error::custom(format!("invalid u64 value '{s}': {err}")))
            }
        }
        Some(other) => Err(D::Error::custom(format!("unexpected numeric value: {other}"))),
    }
}

fn serialize_option_u64_as_string<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(v) => serializer.serialize_str(&v.to_string()),
        None => serializer.serialize_none(),
    }
}

fn deserialize_download_tokens<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let option: Option<Value> = Option::deserialize(deserializer)?;
    let tokens = match option {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            let values: Vec<String> = s
                .split(',')
                .map(|token| token.trim())
                .filter(|token| !token.is_empty())
                .map(|token| token.to_string())
                .collect();
            if values.is_empty() {
                None
            } else {
                Some(values)
            }
        }
        Some(Value::Array(entries)) => {
            let values: Vec<String> = entries
                .into_iter()
                .filter_map(|entry| match entry {
                    Value::String(s) if !s.is_empty() => Some(s),
                    _ => None,
                })
                .collect();
            if values.is_empty() {
                None
            } else {
                Some(values)
            }
        }
        Some(other) => return Err(D::Error::custom(format!("unexpected downloadTokens format: {other}"))),
    };
    Ok(tokens)
}

fn serialize_download_tokens<S>(tokens: &Option<Vec<String>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match tokens {
        Some(values) => serializer.serialize_str(&values.join(",")),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_metadata_from_value() {
        let json = serde_json::json!({
            "bucket": "my-bucket",
            "name": "photos/cat.jpg",
            "fullPath": "photos/cat.jpg",
            "generation": "1",
            "metageneration": "2",
            "size": "42",
            "timeCreated": "2023-01-01T00:00:00Z",
            "updated": "2023-01-01T01:00:00Z",
            "contentType": "image/jpeg",
            "metadata": {"env": "test"},
            "downloadTokens": "token1,token2",
            "md5Hash": "abc",
            "crc32c": "def",
            "etag": "ghi"
        });

        let metadata = ObjectMetadata::from_value(json);
        assert_eq!(metadata.bucket.as_deref(), Some("my-bucket"));
        assert_eq!(metadata.name.as_deref(), Some("photos/cat.jpg"));
        assert_eq!(metadata.size_bytes(), Some(42));
        assert_eq!(metadata.download_tokens().unwrap(), ["token1", "token2"]);
        assert_eq!(metadata.md5_hash.as_deref(), Some("abc"));
        assert_eq!(metadata.crc32c.as_deref(), Some("def"));
        assert_eq!(metadata.etag.as_deref(), Some("ghi"));
    }

    #[test]
    fn serializes_set_metadata_request() {
        let mut request = SetMetadataRequest::new()
            .with_cache_control("max-age=60")
            .with_content_type("image/jpeg")
            .with_md5_hash("abc");
        request.insert_custom_metadata("env", "prod");

        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["cacheControl"], "max-age=60");
        assert_eq!(value["contentType"], "image/jpeg");
        assert_eq!(value["md5Hash"], "abc");
        assert_eq!(value["metadata"]["env"], "prod");
    }
}
