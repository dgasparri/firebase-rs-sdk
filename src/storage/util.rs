pub fn is_url(path: &str) -> bool {
    if let Some(index) = path.find("://") {
        path[..index].chars().all(|ch| ch.is_ascii_alphabetic()) && index > 0
    } else {
        false
    }
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
}
