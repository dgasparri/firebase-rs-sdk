use crate::util::CONSTANTS;

/// Panic with a Firebase-styled internal assertion message when the condition is false.
pub fn assert(condition: bool, message: impl AsRef<str>) {
    if !condition {
        panic!("{}", assertion_error(message));
    }
}

/// Build the string used when throwing assertion errors to keep parity with the JS SDK.
pub fn assertion_error(message: impl AsRef<str>) -> String {
    format!(
        "Firebase ({}) INTERNAL ASSERT FAILED: {}",
        CONSTANTS.sdk_version,
        message.as_ref()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "INTERNAL ASSERT FAILED")]
    fn assert_panics_on_false() {
        assert(false, "should panic");
    }

    #[test]
    fn assertion_error_formats_message() {
        let err = assertion_error("boom");
        assert!(err.contains("Firebase"));
        assert!(err.contains("boom"));
    }
}
