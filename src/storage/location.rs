use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use url::Url;

use crate::storage::constants::DEFAULT_HOST;
use crate::storage::error::{invalid_default_bucket, invalid_url, StorageResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    bucket: String,
    path: String,
}

impl Location {
    pub fn new(bucket: impl Into<String>, path: impl Into<String>) -> Self {
        let bucket = bucket.into();
        let mut path = path.into();
        if !path.is_empty() {
            path = path.trim_start_matches('/').to_string();
            path = path.trim_end_matches('/').to_string();
        }
        Self { bucket, path }
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn is_root(&self) -> bool {
        self.path.is_empty()
    }

    pub fn full_server_url(&self) -> String {
        format!(
            "/b/{}/o/{}",
            utf8_percent_encode(&self.bucket, NON_ALPHANUMERIC),
            utf8_percent_encode(&self.path, NON_ALPHANUMERIC)
        )
    }

    pub fn bucket_only_server_url(&self) -> String {
        format!(
            "/b/{}/o",
            utf8_percent_encode(&self.bucket, NON_ALPHANUMERIC)
        )
    }

    pub fn from_bucket_spec(bucket_spec: &str, host: &str) -> StorageResult<Self> {
        match Self::from_url(bucket_spec, host) {
            Ok(location) if location.is_root() => Ok(location),
            Ok(_) => Err(invalid_default_bucket(bucket_spec)),
            Err(_) => Ok(Self::new(bucket_spec, "")),
        }
    }

    pub fn from_url(url: &str, host: &str) -> StorageResult<Self> {
        if let Some(rest) = url.strip_prefix("gs://") {
            return Self::from_gs_url(rest);
        }

        if url.starts_with("http://") || url.starts_with("https://") {
            return Self::from_http_url(url, host);
        }

        Err(invalid_url(url))
    }

    fn from_gs_url(rest: &str) -> StorageResult<Self> {
        let mut parts = rest.splitn(2, '/');
        let bucket = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid_url(rest))?;
        let path = parts.next().unwrap_or_default();
        Ok(Self::new(bucket, path))
    }

    fn from_http_url(url: &str, configured_host: &str) -> StorageResult<Self> {
        let parsed = Url::parse(url).map_err(|_| invalid_url(url))?;
        let host = parsed.host_str().ok_or_else(|| invalid_url(url))?;
        let is_default_host = configured_host == DEFAULT_HOST;
        let host_matches = host.eq_ignore_ascii_case(configured_host)
            || (is_default_host
                && matches!(host, "storage.googleapis.com" | "storage.cloud.google.com"));
        if !host_matches {
            return Err(invalid_url(url));
        }

        let mut segments = parsed
            .path_segments()
            .ok_or_else(|| invalid_url(url))?
            .map(|segment| segment.to_string())
            .collect::<Vec<_>>();

        // Remove empty trailing segment introduced by trailing slash.
        if segments.last().map_or(false, |s| s.is_empty()) {
            segments.pop();
        }

        if segments.is_empty() {
            return Err(invalid_url(url));
        }

        if host.eq_ignore_ascii_case(configured_host) {
            // Expect /{version}/b/{bucket}/o/{path...}
            if segments.len() < 4 || segments[1] != "b" || segments[3] != "o" {
                return Err(invalid_url(url));
            }
            let bucket = &segments[2];
            let path_segments = &segments[4..];
            let decoded_path = decode_path_segments(path_segments)?;
            return Ok(Self::new(bucket, decoded_path));
        }

        // Cloud storage host variants: /{bucket}/{path...}
        let bucket = segments.first().ok_or_else(|| invalid_url(url))?.clone();
        let decoded_path = decode_path_segments(&segments[1..])?;
        Ok(Self::new(bucket, decoded_path))
    }
}

fn decode_path_segments(segments: &[String]) -> StorageResult<String> {
    let decoded = segments
        .iter()
        .map(|segment| percent_decode_str(segment).decode_utf8_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gs_url() {
        let location = Location::from_url("gs://bucket/path/to/file", DEFAULT_HOST).unwrap();
        assert_eq!(location.bucket(), "bucket");
        assert_eq!(location.path(), "path/to/file");
    }

    #[test]
    fn parses_default_bucket_spec() {
        let location = Location::from_bucket_spec("gs://bucket", DEFAULT_HOST).unwrap();
        assert_eq!(location.bucket(), "bucket");
        assert!(location.is_root());
    }

    #[test]
    fn rejects_bucket_spec_with_path() {
        let err = Location::from_bucket_spec("gs://bucket/obj", DEFAULT_HOST).unwrap_err();
        assert_eq!(err.code_str(), "storage/invalid-default-bucket");
    }

    #[test]
    fn parses_firebase_storage_url() {
        let url = format!(
            "https://{}/v0/b/my-bucket/o/path%2Fto%2Fobject",
            DEFAULT_HOST
        );
        let location = Location::from_url(&url, DEFAULT_HOST).unwrap();
        assert_eq!(location.bucket(), "my-bucket");
        assert_eq!(location.path(), "path/to/object");
    }

    #[test]
    fn parses_cloud_storage_url() {
        let url = "https://storage.googleapis.com/my-bucket/path/to/object";
        let location = Location::from_url(url, DEFAULT_HOST).unwrap();
        assert_eq!(location.bucket(), "my-bucket");
        assert_eq!(location.path(), "path/to/object");
    }
}
