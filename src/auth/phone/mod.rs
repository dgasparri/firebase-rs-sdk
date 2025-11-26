use std::sync::Arc;

use crate::auth::api::Auth;
use crate::auth::error::AuthResult;
use crate::auth::types::{ApplicationVerifier, ConfirmationResult, MultiFactorAssertion};

/// Provider ID for phone authentication.
pub const PHONE_PROVIDER_ID: &str = "phone";

/// Represents a credential produced during a phone verification flow.
#[derive(Clone, Debug)]
pub struct PhoneAuthCredential {
    verification_id: String,
    verification_code: String,
}

impl PhoneAuthCredential {
    /// Creates a credential from a verification ID and SMS verification code.
    pub fn new(verification_id: impl Into<String>, verification_code: impl Into<String>) -> Self {
        Self {
            verification_id: verification_id.into(),
            verification_code: verification_code.into(),
        }
    }

    /// Returns the verification identifier issued by Firebase Auth.
    pub fn verification_id(&self) -> &str {
        &self.verification_id
    }

    /// Returns the SMS verification code supplied by the user.
    pub fn verification_code(&self) -> &str {
        &self.verification_code
    }

    pub(crate) fn into_parts(self) -> (String, String) {
        (self.verification_id, self.verification_code)
    }
}

/// Utility for interacting with phone authentication flows via [`Auth`].
pub struct PhoneAuthProvider {
    auth: Arc<Auth>,
}

impl PhoneAuthProvider {
    /// Creates a provider bound to the supplied [`Auth`] instance.
    pub fn new(auth: Arc<Auth>) -> Self {
        Self { auth }
    }

    /// Sends a verification code to the given phone number and returns the verification ID.
    pub async fn verify_phone_number(
        &self,
        phone_number: &str,
        verifier: Arc<dyn ApplicationVerifier>,
    ) -> AuthResult<String> {
        self.auth.send_phone_verification_code(phone_number, verifier).await
    }

    /// Builds a credential from a verification ID/code pair.
    pub fn credential(verification_id: impl Into<String>, verification_code: impl Into<String>) -> PhoneAuthCredential {
        PhoneAuthCredential::new(verification_id, verification_code)
    }

    /// Derives a credential from a [`ConfirmationResult`].
    pub fn credential_from_confirmation(
        confirmation: &ConfirmationResult,
        verification_code: impl Into<String>,
    ) -> PhoneAuthCredential {
        PhoneAuthCredential::new(confirmation.verification_id(), verification_code.into())
    }

    /// Signs the user in with the provided credential.
    pub async fn sign_in_with_credential(
        &self,
        credential: PhoneAuthCredential,
    ) -> AuthResult<crate::auth::UserCredential> {
        self.auth.sign_in_with_phone_credential(credential).await
    }

    /// Links the current user with the provided credential.
    pub async fn link_with_credential(
        &self,
        credential: PhoneAuthCredential,
    ) -> AuthResult<crate::auth::UserCredential> {
        self.auth.link_with_phone_credential(credential).await
    }

    /// Reauthenticates the current user with the provided credential.
    pub async fn reauthenticate_with_credential(
        &self,
        credential: PhoneAuthCredential,
    ) -> AuthResult<Arc<crate::auth::User>> {
        self.auth.reauthenticate_with_phone_credential(credential).await
    }
}

/// Provides helpers for creating phone-based multi-factor assertions.
///
/// Mirrors the JavaScript implementation in
/// `packages/auth/src/platform_browser/mfa/assertions/phone.ts`.
pub struct PhoneMultiFactorGenerator;

impl PhoneMultiFactorGenerator {
    /// Builds a multi-factor assertion from a [`PhoneAuthCredential`].
    pub fn assertion(credential: PhoneAuthCredential) -> MultiFactorAssertion {
        MultiFactorAssertion::from_phone_credential(credential)
    }

    /// The identifier of the phone second factor (`"phone"`).
    pub const FACTOR_ID: &'static str = PHONE_PROVIDER_ID;
}
