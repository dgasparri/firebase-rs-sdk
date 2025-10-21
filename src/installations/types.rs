use std::time::SystemTime;

/// Represents an authentication token produced by the Firebase Installations service.
///
/// Mirrors the JavaScript type defined in
/// `packages/installations/src/interfaces/installation-entry.ts`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationToken {
    pub token: String,
    pub expires_at: SystemTime,
}

impl InstallationToken {
    /// Returns `true` if the token has already expired.
    pub fn is_expired(&self) -> bool {
        SystemTime::now() >= self.expires_at
    }
}
