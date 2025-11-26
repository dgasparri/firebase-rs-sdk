use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use percent_encoding::percent_decode_str;

use crate::storage::error::{invalid_argument, StorageResult};

/// Mirrors the Firebase Web SDK string upload formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringFormat {
    /// Interpret the input as UTF-8 text.
    Raw,
    /// Interpret the input as standard base64 encoded data.
    Base64,
    /// Interpret the input as base64url encoded data.
    Base64Url,
    /// Interpret the input as a data URL (e.g. `data:image/png;base64,...`).
    DataUrl,
}

impl Default for StringFormat {
    fn default() -> Self {
        StringFormat::Raw
    }
}

#[derive(Debug)]
pub struct PreparedString {
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
}

impl PreparedString {
    fn new(bytes: Vec<u8>, content_type: Option<String>) -> Self {
        Self { bytes, content_type }
    }
}

pub fn prepare_string_upload(value: &str, format: StringFormat) -> StorageResult<PreparedString> {
    match format {
        StringFormat::Raw => Ok(PreparedString::new(value.as_bytes().to_vec(), None)),
        StringFormat::Base64 => decode_base64(value),
        StringFormat::Base64Url => decode_base64_url(value),
        StringFormat::DataUrl => decode_data_url(value),
    }
}

fn decode_base64(value: &str) -> StorageResult<PreparedString> {
    STANDARD
        .decode(value)
        .map(|bytes| PreparedString::new(bytes, None))
        .map_err(|err| invalid_argument(format!("Invalid base64 data: {err}")))
}

fn decode_base64_url(value: &str) -> StorageResult<PreparedString> {
    let sanitized = value.trim_end_matches('=');
    URL_SAFE_NO_PAD
        .decode(sanitized)
        .map(|bytes| PreparedString::new(bytes, None))
        .map_err(|err| invalid_argument(format!("Invalid base64url data: {err}")))
}

fn decode_data_url(value: &str) -> StorageResult<PreparedString> {
    if !value.starts_with("data:") {
        return Err(invalid_argument("Data URL must start with the 'data:' scheme."));
    }

    let comma = value
        .find(',')
        .ok_or_else(|| invalid_argument("Data URL must contain a comma separating metadata and data segments."))?;

    let metadata = &value[5..comma];
    let data_part = &value[comma + 1..];

    let (is_base64, content_type) = if metadata.is_empty() {
        (false, None)
    } else if let Some(stripped) = metadata.strip_suffix(";base64") {
        let content_type = stripped.trim();
        let content_type = if content_type.is_empty() {
            None
        } else {
            Some(content_type.to_string())
        };
        (true, content_type)
    } else {
        let content_type = metadata.trim();
        let content_type = if content_type.is_empty() {
            None
        } else {
            Some(content_type.to_string())
        };
        (false, content_type)
    };

    let bytes = if is_base64 {
        STANDARD
            .decode(data_part)
            .map_err(|err| invalid_argument(format!("Invalid base64 data URL: {err}")))?
    } else {
        percent_decode_str(data_part)
            .decode_utf8()
            .map_err(|_| invalid_argument("Data URL payload must be valid percent-encoded UTF-8."))?
            .into_owned()
            .into_bytes()
    };

    Ok(PreparedString::new(bytes, content_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_returns_utf8_bytes() {
        let prepared = prepare_string_upload("hello", StringFormat::Raw).unwrap();
        assert_eq!(prepared.bytes, b"hello");
        assert!(prepared.content_type.is_none());
    }

    #[test]
    fn base64_decodes_to_bytes() {
        let prepared = prepare_string_upload("aGVsbG8=", StringFormat::Base64).unwrap();
        assert_eq!(prepared.bytes, b"hello");
    }

    #[test]
    fn base64_url_allows_paddingless_values() {
        let prepared = prepare_string_upload("aGVsbG8", StringFormat::Base64Url).unwrap();
        assert_eq!(prepared.bytes, b"hello");
    }

    #[test]
    fn data_url_extracts_content_type() {
        let prepared = prepare_string_upload("data:text/plain;base64,aGVsbG8=", StringFormat::DataUrl).unwrap();
        assert_eq!(prepared.bytes, b"hello");
        assert_eq!(prepared.content_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn data_url_percent_encoded() {
        let prepared = prepare_string_upload("data:,Hello%20World", StringFormat::DataUrl).unwrap();
        assert_eq!(prepared.bytes, b"Hello World");
        assert!(prepared.content_type.is_none());
    }
}
