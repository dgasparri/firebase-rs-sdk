use base64::Engine;
use rand::Rng;
use sha2::{Digest, Sha256};

const PKCE_LENGTH: usize = 64;
const PKCE_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// Represents a PKCE verifier/challenge pair ready to attach to OAuth flows.
#[derive(Debug, Clone)]
pub struct PkcePair {
    code_verifier: String,
    code_challenge: String,
}

impl PkcePair {
    /// Generates a new PKCE pair using a cryptographically secure RNG.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let verifier: String = (0..PKCE_LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..PKCE_CHARSET.len());
                PKCE_CHARSET[idx] as char
            })
            .collect();

        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

        Self {
            code_verifier: verifier,
            code_challenge: challenge,
        }
    }

    /// Returns the plain-text code verifier value that must be sent during token exchange.
    pub fn code_verifier(&self) -> &str {
        &self.code_verifier
    }

    /// Returns the base64url-encoded code challenge.
    pub fn code_challenge(&self) -> &str {
        &self.code_challenge
    }

    /// Returns the PKCE method (currently always `S256`).
    pub fn method(&self) -> &'static str {
        "S256"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_pair_has_expected_lengths() {
        let pkce = PkcePair::generate();
        assert!(pkce.code_verifier().len() >= 43);
        assert!(pkce.code_verifier().len() <= 128);
        assert!(!pkce.code_challenge().is_empty());
        assert_eq!(pkce.method(), "S256");
    }
}
