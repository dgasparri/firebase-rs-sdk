//! Remote Config settings surface.
//!
//! Mirrors the configuration exposed by the Firebase JS SDK (`RemoteConfigSettings`) with
//! validation tailored for the Rust API.

use crate::remote_config::error::{invalid_argument, RemoteConfigResult};

/// Default timeout for fetch operations (60 seconds).
pub const DEFAULT_FETCH_TIMEOUT_MILLIS: u64 = 60_000;
/// Default minimum interval between successful fetches (12 hours).
pub const DEFAULT_MINIMUM_FETCH_INTERVAL_MILLIS: u64 = 12 * 60 * 60 * 1_000;

/// Configuration options for Remote Config fetch behaviour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteConfigSettings {
    fetch_timeout_millis: u64,
    minimum_fetch_interval_millis: u64,
}

impl RemoteConfigSettings {
    /// Creates a new settings object after validating values.
    pub fn new(fetch_timeout_millis: u64, minimum_fetch_interval_millis: u64) -> RemoteConfigResult<Self> {
        validate_fetch_timeout(fetch_timeout_millis)?;
        validate_minimum_fetch_interval(minimum_fetch_interval_millis)?;
        Ok(Self {
            fetch_timeout_millis,
            minimum_fetch_interval_millis,
        })
    }

    /// Returns the fetch timeout in milliseconds.
    pub fn fetch_timeout_millis(&self) -> u64 {
        self.fetch_timeout_millis
    }

    /// Returns the minimum fetch interval in milliseconds.
    pub fn minimum_fetch_interval_millis(&self) -> u64 {
        self.minimum_fetch_interval_millis
    }

    pub(crate) fn set_fetch_timeout_millis(&mut self, value: u64) -> RemoteConfigResult<()> {
        validate_fetch_timeout(value)?;
        self.fetch_timeout_millis = value;
        Ok(())
    }

    pub(crate) fn set_minimum_fetch_interval_millis(&mut self, value: u64) -> RemoteConfigResult<()> {
        validate_minimum_fetch_interval(value)?;
        self.minimum_fetch_interval_millis = value;
        Ok(())
    }
}

impl Default for RemoteConfigSettings {
    fn default() -> Self {
        Self {
            fetch_timeout_millis: DEFAULT_FETCH_TIMEOUT_MILLIS,
            minimum_fetch_interval_millis: DEFAULT_MINIMUM_FETCH_INTERVAL_MILLIS,
        }
    }
}

/// Partial update to apply on top of existing settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteConfigSettingsUpdate {
    pub fetch_timeout_millis: Option<u64>,
    pub minimum_fetch_interval_millis: Option<u64>,
}

impl RemoteConfigSettingsUpdate {
    pub fn is_empty(&self) -> bool {
        self.fetch_timeout_millis.is_none() && self.minimum_fetch_interval_millis.is_none()
    }
}

pub(crate) fn validate_fetch_timeout(value: u64) -> RemoteConfigResult<()> {
    if value == 0 {
        return Err(invalid_argument("fetch_timeout_millis must be greater than zero"));
    }
    Ok(())
}

pub(crate) fn validate_minimum_fetch_interval(_value: u64) -> RemoteConfigResult<()> {
    // The JS SDK accepts zero to disable throttling; non-negative constraint is encoded in the type.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_align_with_js_sdk() {
        let defaults = RemoteConfigSettings::default();
        assert_eq!(defaults.fetch_timeout_millis(), DEFAULT_FETCH_TIMEOUT_MILLIS);
        assert_eq!(defaults.minimum_fetch_interval_millis(), DEFAULT_MINIMUM_FETCH_INTERVAL_MILLIS);
    }

    #[test]
    fn new_validates_fetch_timeout() {
        assert!(RemoteConfigSettings::new(1, 0).is_ok());
        let err = RemoteConfigSettings::new(0, 0).unwrap_err();
        assert_eq!(err.code_str(), "remote-config/invalid-argument");
    }

    #[test]
    fn update_is_empty_when_no_values() {
        let update = RemoteConfigSettingsUpdate::default();
        assert!(update.is_empty());
    }
}
