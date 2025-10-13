/// Firebase constants ported from the JS util package.
#[derive(Debug, Clone, Copy)]
pub struct Constants {
    pub node_client: bool,
    pub node_admin: bool,
    pub sdk_version: &'static str,
}

/// Static constants for the Rust port. Node flags default to false; tweak as needed downstream.
pub const CONSTANTS: Constants = Constants {
    node_client: false,
    node_admin: false,
    sdk_version: env!("CARGO_PKG_VERSION"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_version_matches_crate_version() {
        assert_eq!(CONSTANTS.sdk_version, env!("CARGO_PKG_VERSION"));
    }
}
