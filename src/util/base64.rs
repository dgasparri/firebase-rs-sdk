use base64::engine::general_purpose::URL_SAFE;
use base64::engine::Engine as _;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeBase64Error;

impl fmt::Display for DecodeBase64Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to decode base64 string")
    }
}

impl std::error::Error for DecodeBase64Error {}

/// Encode a string using a URL-safe base64 alphabet with `.` padding to match the JS SDK.
pub fn base64_encode(input: &str) -> String {
    base64_url_encode(input)
}

/// Encode a string using the JS SDK web-safe alphabet.
pub fn base64_url_encode(input: &str) -> String {
    base64_url_encode_bytes(input.as_bytes())
}

/// Encode a byte slice using the JS SDK web-safe alphabet (padding replaced with `.`).
pub fn base64_url_encode_bytes(bytes: &[u8]) -> String {
    let encoded = URL_SAFE.encode(bytes);
    encoded.replace('=', ".")
}

/// Encode and keep the JS SDK behaviour of trimming trailing padding characters.
pub fn base64_url_encode_trimmed(input: &str) -> String {
    base64_encode(input).trim_end_matches('.').to_owned()
}

/// Decode a base64 URL-safe string, returning UTF-8 text on success.
pub fn base64_decode(input: &str) -> Result<String, DecodeBase64Error> {
    let bytes = base64_decode_bytes(input)?;
    String::from_utf8(bytes).map_err(|_err| DecodeBase64Error)
}

/// Decode into raw bytes.
pub fn base64_decode_bytes(input: &str) -> Result<Vec<u8>, DecodeBase64Error> {
    let mut normalized = input.replace('.', "=");
    // The JS SDK allows the padding to be absent. Restore padding so the decoder accepts it.
    let remainder = normalized.len() % 4;
    if remainder != 0 {
        normalized.extend("====".chars().take(4 - remainder));
    }
    URL_SAFE
        .decode(normalized.as_bytes())
        .map_err(|_err| DecodeBase64Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_decode_roundtrip() {
        let original = "hello firebase";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn encode_trimmed_removes_padding() {
        let trimmed = base64_url_encode_trimmed("test");
        assert!(!trimmed.ends_with('.'));
    }

    #[test]
    fn decode_tolerates_missing_padding() {
        let encoded = base64_encode("data");
        let without_padding = encoded.trim_end_matches('.');
        let decoded = base64_decode(without_padding).unwrap();
        assert_eq!(decoded, "data");
    }

    #[test]
    fn decode_invalid_returns_error() {
        assert!(base64_decode("@@invalid@@").is_err());
    }
}
