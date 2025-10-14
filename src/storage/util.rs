pub fn is_url(path: &str) -> bool {
    if let Some(index) = path.find("://") {
        path[..index].chars().all(|ch| ch.is_ascii_alphabetic()) && index > 0
    } else {
        false
    }
}

pub fn is_retry_status_code(status: u16, additional: &[u16]) -> bool {
    (500..600).contains(&status) || matches!(status, 408 | 429) || additional.contains(&status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_urls() {
        assert!(is_url("gs://bucket/path"));
        assert!(is_url("https://example.com"));
        assert!(!is_url("not/a/url"));
        assert!(!is_url("://missing"));
    }

    #[test]
    fn retry_status_codes() {
        assert!(is_retry_status_code(500, &[]));
        assert!(is_retry_status_code(408, &[]));
        assert!(!is_retry_status_code(404, &[]));
        assert!(is_retry_status_code(499, &[499]));
    }
}
