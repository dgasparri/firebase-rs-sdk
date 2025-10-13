use std::convert::TryFrom;

use serde_json::Value;
use url::form_urlencoded::Serializer;

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::AuthCredential;

/// Wraps the OAuth credential payload returned by popup/redirect handlers.
///
/// The credential format mirrors the JS SDK: the handler supplies a JSON blob
/// describing the provider response (ID token, access token, pending post body).
/// `OAuthCredential` exposes helpers to build the `postBody` required by the
/// `signInWithIdp` REST endpoint.
#[derive(Debug, Clone)]
pub struct OAuthCredential {
    provider_id: String,
    sign_in_method: String,
    raw_nonce: Option<String>,
    token_response: Value,
}

impl OAuthCredential {
    pub fn new(
        provider_id: impl Into<String>,
        sign_in_method: impl Into<String>,
        token_response: Value,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            sign_in_method: sign_in_method.into(),
            raw_nonce: None,
            token_response,
        }
    }

    pub fn with_raw_nonce(mut self, nonce: Option<String>) -> Self {
        self.raw_nonce = nonce;
        self
    }

    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub fn sign_in_method(&self) -> &str {
        &self.sign_in_method
    }

    pub fn token_response(&self) -> &Value {
        &self.token_response
    }

    pub fn raw_nonce(&self) -> Option<&String> {
        self.raw_nonce.as_ref()
    }

    /// Builds the `postBody` query string expected by `signInWithIdp`.
    pub fn build_post_body(&self) -> AuthResult<String> {
        let mut serializer = Serializer::new(String::new());
        let mut has_credential = false;

        if let Some(id_token) = self
            .token_response
            .get("idToken")
            .or_else(|| self.token_response.get("oauthIdToken"))
            .and_then(Value::as_str)
        {
            serializer.append_pair("id_token", id_token);
            has_credential = true;
        }

        if let Some(access_token) = self
            .token_response
            .get("accessToken")
            .or_else(|| self.token_response.get("oauthAccessToken"))
            .and_then(Value::as_str)
        {
            serializer.append_pair("access_token", access_token);
            has_credential = true;
        }

        if let Some(code) = self.token_response.get("code").and_then(Value::as_str) {
            serializer.append_pair("code", code);
            has_credential = true;
        }

        if !has_credential {
            return Err(AuthError::InvalidCredential(
                "OAuth token response missing id_token/access_token/code".into(),
            ));
        }

        if let Some(nonce) = self.raw_nonce() {
            serializer.append_pair("nonce", nonce);
        }

        serializer.append_pair("providerId", &self.provider_id);

        Ok(serializer.finish())
    }
}

impl TryFrom<AuthCredential> for OAuthCredential {
    type Error = AuthError;

    fn try_from(credential: AuthCredential) -> Result<Self, Self::Error> {
        let raw_nonce = credential
            .token_response
            .get("nonce")
            .and_then(Value::as_str)
            .map(|value| value.to_owned());

        Ok(Self {
            provider_id: credential.provider_id,
            sign_in_method: credential.sign_in_method,
            raw_nonce,
            token_response: credential.token_response,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn post_body_includes_id_token() {
        let credential = AuthCredential {
            provider_id: "google.com".into(),
            sign_in_method: "google.com".into(),
            token_response: json!({ "idToken": "test-id-token" }),
        };

        let oauth = OAuthCredential::try_from(credential).unwrap();
        let result = oauth.build_post_body().unwrap();
        assert!(result.contains("id_token=test-id-token"));
        assert!(result.contains("providerId=google.com"));
    }

    #[test]
    fn post_body_includes_access_token() {
        let credential = AuthCredential {
            provider_id: "github.com".into(),
            sign_in_method: "github.com".into(),
            token_response: json!({ "accessToken": "gh-token" }),
        };

        let oauth = OAuthCredential::try_from(credential).unwrap();
        let result = oauth.build_post_body().unwrap();
        assert!(result.contains("access_token=gh-token"));
        assert!(result.contains("providerId=github.com"));
    }

    #[test]
    fn post_body_errors_when_missing_tokens() {
        let credential = AuthCredential {
            provider_id: "google.com".into(),
            sign_in_method: "google.com".into(),
            token_response: json!({}),
        };

        let oauth = OAuthCredential::try_from(credential).unwrap();
        let err = oauth.build_post_body().unwrap_err();
        assert!(matches!(err, AuthError::InvalidCredential(_)));
    }
}
