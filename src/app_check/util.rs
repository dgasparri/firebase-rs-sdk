use std::time::Duration;

use crate::app_check::errors::{AppCheckError, AppCheckResult};

/// Parses a protobuf-style duration string (e.g. "3600s") into a [`Duration`].
pub fn parse_protobuf_duration(value: &str) -> AppCheckResult<Duration> {
    let Some(raw_seconds) = value.strip_suffix('s') else {
        return Err(AppCheckError::FetchParseError {
            message: format!("ttl '{value}' is missing the 's' suffix"),
        });
    };

    let seconds: f64 = raw_seconds
        .parse()
        .map_err(|err| AppCheckError::FetchParseError {
            message: format!("failed to parse ttl '{value}': {err}"),
        })?;

    if seconds.is_sign_negative() {
        return Err(AppCheckError::FetchParseError {
            message: format!("ttl '{value}' must be positive"),
        });
    }

    let millis = (seconds * 1000.0).round();
    if millis < 0.0 || millis > u64::MAX as f64 {
        return Err(AppCheckError::FetchParseError {
            message: format!("ttl '{value}' exceeds supported range"),
        });
    }

    Ok(Duration::from_millis(millis as u64))
}

/// Formats a duration into a compact "1d:02h:03m:04s" string mirroring the JS SDK.
pub fn format_duration(duration: Duration) -> String {
    let total_seconds = (duration.as_millis() as f64 / 1000.0).round() as u64;
    let days = total_seconds / (24 * 3600);
    let hours = (total_seconds % (24 * 3600)) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    let mut parts = String::new();
    if days > 0 {
        parts.push_str(&format!("{}d:", pad(days)));
    }
    if hours > 0 {
        parts.push_str(&format!("{}h:", pad(hours)));
    }

    parts.push_str(&format!("{}m:{}s", pad(minutes), pad(seconds)));
    parts
}

fn pad(value: u64) -> String {
    if value >= 10 {
        value.to_string()
    } else {
        format!("0{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proto_duration() {
        let duration = parse_protobuf_duration("3600s").unwrap();
        assert_eq!(duration.as_secs(), 3600);

        let duration = parse_protobuf_duration("1.5s").unwrap();
        assert_eq!(duration.as_millis(), 1500);
    }

    #[test]
    fn rejects_invalid_proto_duration() {
        assert!(parse_protobuf_duration("3600").is_err());
        assert!(parse_protobuf_duration("-1s").is_err());
    }

    #[test]
    fn formats_duration_like_js() {
        assert_eq!(format_duration(Duration::from_secs(65)), "01m:05s");
        assert_eq!(format_duration(Duration::from_secs(3605)), "01h:00m:05s");
        assert_eq!(
            format_duration(Duration::from_secs(90061)),
            "01d:01h:01m:01s"
        );
    }
}
