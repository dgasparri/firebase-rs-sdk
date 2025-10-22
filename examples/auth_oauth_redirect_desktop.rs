use std::fs;
use std::sync::Arc;

use firebase_rs_sdk::app::FirebaseApp;
use firebase_rs_sdk::auth::*;
use serde_json::Value;

struct DesktopRedirectHandler;

impl OAuthRedirectHandler for DesktopRedirectHandler {
    fn initiate_redirect(&self, request: OAuthRequest) -> AuthResult<()> {
        println!("Opening system browser for {}", request.provider_id);
        webbrowser::open(&request.auth_url).map_err(|err| AuthError::Network(err.to_string()))?;
        Ok(())
    }

    fn complete_redirect(&self) -> AuthResult<Option<AuthCredential>> {
        let path = match std::env::var("OAUTH_CREDENTIAL_PATH") {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };

        let payload = fs::read_to_string(path)
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))?;
        let token_response: Value = serde_json::from_str(&payload)
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))?;

        let provider_id = token_response
            .get("providerId")
            .and_then(Value::as_str)
            .unwrap_or("custom");

        Ok(Some(AuthCredential {
            provider_id: provider_id.to_string(),
            sign_in_method: provider_id.to_string(),
            token_response,
        }))
    }
}

fn configure_provider() -> OAuthProvider {
    let mut provider = OAuthProvider::new("github.com", "https://github.com/login/oauth/authorize");
    provider.add_scope("read:user");
    provider
}

fn main() -> AuthResult<()> {
    let _app: FirebaseApp = todo!("Initialize Firebase app with your configuration");
    #[allow(unreachable_code)]
    let auth = Auth::builder(_app)
        .with_redirect_handler(Arc::new(DesktopRedirectHandler))
        .with_oauth_request_uri("http://localhost")
        .build()?;

    let provider = configure_provider();
    provider.sign_in_with_redirect(&auth)?;

    // Later, once the redirect completes and the credential payload is available:
    if let Some(credential) = OAuthProvider::get_redirect_result(&auth)? {
        println!("Signed in with provider {:?}", credential.provider_id);
    }

    Ok(())
}
