#[cfg(target_arch = "wasm32")]
mod wasm_demo {
    use std::sync::Arc;

    use firebase_rs_sdk::app::FirebaseApp;
    use firebase_rs_sdk::auth::*;
    use js_sys::Promise;
    use serde_json::{json, Value};
    use serde_wasm_bindgen::{from_value, to_value};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::{spawn_local, JsFuture};

    struct JsPopupHandler;

    impl OAuthPopupHandler for JsPopupHandler {
        fn open_popup(&self, request: OAuthRequest) -> AuthResult<AuthCredential> {
            let js_value = bindings::open_popup_via_js(&request.auth_url)
                .map_err(|err| AuthError::InvalidCredential(js_error_to_string(err)))?;
            let token_response: Value = from_value(js_value)
                .map_err(|err| AuthError::InvalidCredential(err.to_string()))?;

            Ok(AuthCredential {
                provider_id: request.provider_id.clone(),
                sign_in_method: request.provider_id,
                token_response,
            })
        }
    }

    mod bindings {
        use super::*;

        #[wasm_bindgen(module = "supporting_js/auth_oauth_popup_wasm-auth_popup.js")]
        extern "C" {
            #[wasm_bindgen(catch)]
            pub fn open_popup_via_js(url: &str) -> Result<JsValue, JsValue>;

            #[wasm_bindgen(catch)]
            pub fn start_passkey_conditional_ui(request: JsValue) -> Result<Promise, JsValue>;
        }
    }

    fn js_error_to_string(value: JsValue) -> String {
        if let Some(text) = value.as_string() {
            return text;
        }
        if let Ok(text) = js_sys::JSON::stringify(&value) {
            if let Some(text) = text.as_string() {
                return text;
            }
        }
        format!("{value:?}")
    }

    fn configure_provider() -> OAuthProvider {
        let mut provider =
            OAuthProvider::new("google.com", "https://accounts.google.com/o/oauth2/v2/auth");
        provider.add_scope("profile");
        provider.add_scope("email");
        provider.enable_pkce();
        provider
    }

    fn initialize_auth() -> AuthResult<Arc<Auth>> {
        let app: FirebaseApp = todo!("Initialize Firebase app in WASM host (call initialize_app)");
        Auth::builder(app)
            .with_popup_handler(Arc::new(JsPopupHandler))
            .with_oauth_request_uri("http://localhost")
            .build()
    }

    #[wasm_bindgen(start)]
    pub fn start() -> Result<(), JsValue> {
        let auth = initialize_auth().map_err(|err| JsValue::from_str(&err.to_string()))?;
        let provider = configure_provider();

        // This will panic until the surrounding JS glue returns a valid credential payload.
        let auth_clone = auth.clone();
        let provider_clone = provider.clone();
        spawn_local(async move {
            if let Err(err) = provider_clone.sign_in_with_popup(&auth_clone).await {
                web_sys::console::error_1(&JsValue::from_str(&err.to_string()));
            }
        });

        // Demonstrates how a host app could forward passkey conditional UI results back to the resolver.
        // The helper is unused by default because it requires a Firebase backend to produce challenges.
        #[allow(dead_code)]
        async fn resolve_passkey_with_conditional_ui(
            resolver: MultiFactorResolver,
        ) -> AuthResult<UserCredential> {
            let hint = resolver
                .hints()
                .iter()
                .find(|info| info.factor_id == WEBAUTHN_FACTOR_ID)
                .cloned()
                .ok_or_else(|| {
                    AuthError::InvalidCredential(
                        "Resolver does not include a WebAuthn passkey factor".into(),
                    )
                })?;

            let challenge = resolver.start_passkey_sign_in(&hint).await?;

            let allow_credentials = challenge
                .allow_credentials()
                .into_iter()
                .map(|descriptor| {
                    json!({
                        "type": descriptor.credential_type(),
                        "id": descriptor.id(),
                        "transports": descriptor
                            .transports()
                            .iter()
                            .map(|transport| transport.as_str().to_string())
                            .collect::<Vec<String>>(),
                    })
                })
                .collect::<Vec<_>>();

            let request_payload = json!({
                "challenge": challenge.challenge(),
                "rpId": challenge.rp_id(),
                "allowCredentials": allow_credentials,
                "mediation": "conditional",
            });

            let js_request = to_value(&request_payload)
                .map_err(|err| AuthError::InvalidCredential(err.to_string()))?;
            let promise = bindings::start_passkey_conditional_ui(js_request)
                .map_err(|err| AuthError::InvalidCredential(js_error_to_string(err)))?;
            let assertion_value = JsFuture::from(promise)
                .await
                .map_err(|err| AuthError::InvalidCredential(js_error_to_string(err)))?;
            let assertion_json: Value = from_value(assertion_value)
                .map_err(|err| AuthError::InvalidCredential(err.to_string()))?;

            let assertion_response = WebAuthnAssertionResponse::try_from(assertion_json)?;
            let assertion = WebAuthnMultiFactorGenerator::assertion_for_sign_in(
                hint.uid.clone(),
                assertion_response,
            );
            resolver.resolve_sign_in(assertion).await
        }
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    panic!("Compile this example for --target wasm32-unknown-unknown and --features wasm-web)");
}
