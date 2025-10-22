//! Remote Config value helpers mirroring the Firebase JS SDK implementation.
//!
//! Based on the logic in `packages/remote-config/src/value.ts` which normalises
//! Remote Config parameter values and exposes typed accessors.

/// Indicates where a Remote Config value originated from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteConfigValueSource {
    /// Value fetched from the Remote Config backend and activated.
    Remote,
    /// Default value supplied by the client via `set_defaults`.
    Default,
    /// Static fallback used when the key has no remote or default entry.
    Static,
}

impl RemoteConfigValueSource {
    /// Returns the string identifier used in the JS SDK (`remote`, `default`, or `static`).
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteConfigValueSource::Remote => "remote",
            RemoteConfigValueSource::Default => "default",
            RemoteConfigValueSource::Static => "static",
        }
    }
}

/// Represents a Remote Config parameter value with typed accessors.
///
/// This follows the behaviour of the JavaScript `Value` class where missing keys map to a static
/// source with empty string defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteConfigValue {
    source: RemoteConfigValueSource,
    value: String,
}

impl RemoteConfigValue {
    const DEFAULT_BOOLEAN: bool = false;
    const DEFAULT_NUMBER: f64 = 0.0;
    const BOOLEAN_TRUTHY_VALUES: [&'static str; 6] = ["1", "true", "t", "yes", "y", "on"];

    pub(crate) fn new(source: RemoteConfigValueSource, value: impl Into<String>) -> Self {
        Self {
            source,
            value: value.into(),
        }
    }

    pub(crate) fn static_value() -> Self {
        Self::new(RemoteConfigValueSource::Static, String::new())
    }

    /// Returns the raw value as a string.
    ///
    /// # Examples
    ///
    /// ```
    /// use firebase_rs_sdk::remote_config::value::RemoteConfigValue;
    ///
    /// let value = RemoteConfigValue::default();
    /// assert_eq!(value.as_string(), "");
    /// ```
    pub fn as_string(&self) -> String {
        self.value.clone()
    }

    /// Returns the value interpreted as a boolean.
    ///
    /// Matches the JS SDK semantics: `true` for case-insensitive values in
    /// `{"1", "true", "t", "yes", "y", "on"}` when the source is remote/default, otherwise `false`.
    pub fn as_bool(&self) -> bool {
        if self.source == RemoteConfigValueSource::Static {
            return Self::DEFAULT_BOOLEAN;
        }
        Self::BOOLEAN_TRUTHY_VALUES
            .iter()
            .any(|truthy| self.value.eq_ignore_ascii_case(truthy))
    }

    /// Returns the value interpreted as a number.
    ///
    /// Parsing failures fall back to `0.0`, mirroring the JavaScript implementation.
    pub fn as_number(&self) -> f64 {
        if self.source == RemoteConfigValueSource::Static {
            return Self::DEFAULT_NUMBER;
        }
        match self.value.trim().parse::<f64>() {
            Ok(parsed) if parsed.is_finite() || parsed == 0.0 => parsed,
            Ok(parsed) if parsed.is_nan() => Self::DEFAULT_NUMBER,
            Ok(parsed) => parsed,
            Err(_) => Self::DEFAULT_NUMBER,
        }
    }

    /// Returns the source of the value (`remote`, `default`, `static`).
    pub fn source(&self) -> RemoteConfigValueSource {
        self.source.clone()
    }
}

impl Default for RemoteConfigValue {
    fn default() -> Self {
        Self::static_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boolean_truthy_values_match_js_behaviour() {
        for truthy in RemoteConfigValue::BOOLEAN_TRUTHY_VALUES {
            let value = RemoteConfigValue::new(RemoteConfigValueSource::Remote, truthy);
            assert!(
                value.as_bool(),
                "expected truthy value {} to be true",
                truthy
            );
        }
        let value = RemoteConfigValue::new(RemoteConfigValueSource::Remote, "false");
        assert!(!value.as_bool());
    }

    #[test]
    fn static_boolean_is_false() {
        let value = RemoteConfigValue::static_value();
        assert!(!value.as_bool());
    }

    #[test]
    fn number_parsing_matches_js_defaults() {
        let value = RemoteConfigValue::new(RemoteConfigValueSource::Default, "42.5");
        assert_eq!(value.as_number(), 42.5);

        let invalid = RemoteConfigValue::new(RemoteConfigValueSource::Remote, "NaN");
        assert_eq!(invalid.as_number(), 0.0);

        let static_value = RemoteConfigValue::static_value();
        assert_eq!(static_value.as_number(), 0.0);
    }
}
