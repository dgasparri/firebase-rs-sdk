#[cfg(target_arch = "wasm32")]
mod wasm_demo {
    use std::sync::Arc;

    use firebase_rs_sdk::app::FirebaseApp;
    use firebase_rs_sdk::auth::*;
    use serde_json::Value;
    use serde_wasm_bindgen::from_value;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::spawn_local;

    struct JsPopupHandler;

    impl OAuthPopupHandler for JsPopupHandler {
        fn open_popup(&self, request: OAuthRequest) -> AuthResult<AuthCredential> {
            let js_value = open_popup_via_js(&request.auth_url)
                .map_err(|err| AuthError::InvalidCredential(js_error_to_string(err)))?;
            let token_response: Value = from_value(js_value)
                .map_err(|err| AuthError::InvalidCredential(err.to_string()))?;

            Ok(AuthCredential {
                provider_id: request.provider_id,
                sign_in_method: request.provider_id,
                token_response,
            })
        }
    }

    #[wasm_bindgen(module = "/js/auth_popup.js")]
    extern "C" {
        #[wasm_bindgen(catch)]
        fn open_popup_via_js(url: &str) -> Result<JsValue, JsValue>;
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
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    panic!("Compile this example for wasm32-unknown-unknown (requires --features wasm-web)");
}
