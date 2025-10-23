use serde::{Deserialize, Serialize};

use crate::messaging::error::{
    token_subscribe_failed, token_subscribe_no_token, token_update_failed, token_update_no_token,
    MessagingResult,
};

pub const FCM_API_URL: &str = "https://fcmregistrations.googleapis.com/v1";

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
#[derive(Debug, Clone)]
pub struct FcmRegistrationRequest<'a> {
    pub project_id: &'a str,
    pub api_key: &'a str,
    pub installation_auth_token: &'a str,
    pub subscription: FcmSubscription<'a>,
}

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
#[derive(Debug, Clone)]
pub struct FcmSubscription<'a> {
    pub endpoint: &'a str,
    pub auth: &'a str,
    pub p256dh: &'a str,
    pub application_pub_key: Option<&'a str>,
}

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
#[derive(Debug, Clone)]
pub struct FcmUpdateRequest<'a> {
    pub registration_token: &'a str,
    pub registration: FcmRegistrationRequest<'a>,
}

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::FcmClient;

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
mod wasm;
#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
pub use wasm::FcmClient;

#[cfg(all(target_arch = "wasm32", not(feature = "wasm-web")))]
compile_error!(
    "Building firebase-rs-sdk for wasm32 requires enabling the `wasm-web` feature for the messaging module."
);

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RegistrationRequestBody<'a> {
    web: RegistrationWebBody<'a>,
}

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RegistrationWebBody<'a> {
    endpoint: &'a str,
    auth: &'a str,
    p256dh: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    application_pub_key: Option<&'a str>,
}

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
#[derive(Deserialize)]
struct FcmResponse {
    token: Option<String>,
    #[serde(default)]
    error: Option<FcmErrorBody>,
}

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
#[derive(Deserialize)]
struct FcmErrorBody {
    message: String,
}

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
fn build_body<'a>(subscription: &FcmSubscription<'a>) -> RegistrationRequestBody<'a> {
    RegistrationRequestBody {
        web: RegistrationWebBody {
            endpoint: subscription.endpoint,
            auth: subscription.auth,
            p256dh: subscription.p256dh,
            application_pub_key: subscription.application_pub_key,
        },
    }
}

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
fn map_subscribe_response(response: FcmResponse) -> MessagingResult<String> {
    if let Some(error) = response.error {
        return Err(token_subscribe_failed(error.message));
    }
    response.token.ok_or_else(token_subscribe_no_token)
}

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
fn map_update_response(response: FcmResponse) -> MessagingResult<String> {
    if let Some(error) = response.error {
        return Err(token_update_failed(error.message));
    }
    response.token.ok_or_else(token_update_no_token)
}

#[cfg_attr(
    not(any(test, all(feature = "wasm-web", target_arch = "wasm32"))),
    allow(dead_code)
)]
fn build_headers(
    api_key: &str,
    installation_auth_token: &str,
) -> MessagingResult<Vec<(String, String)>> {
    Ok(vec![
        ("Content-Type".into(), "application/json".into()),
        ("Accept".into(), "application/json".into()),
        ("x-goog-api-key".into(), api_key.to_string()),
        (
            "x-goog-firebase-installations-auth".into(),
            format!("FIS {installation_auth_token}"),
        ),
    ])
}

#[cfg(test)]
mod tests;
