use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::installations::error::{invalid_argument, InstallationsResult};
use crate::installations::types::InstallationToken;

pub const INSTALLATIONS_API_URL: &str = "https://firebaseinstallations.googleapis.com/v1";
const INTERNAL_AUTH_VERSION: &str = "FIS_v2";
const SDK_VERSION: &str = concat!("w:", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredInstallation {
    pub fid: String,
    pub refresh_token: String,
    pub auth_token: InstallationToken,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateInstallationRequest<'a> {
    fid: &'a str,
    auth_version: &'static str,
    app_id: &'a str,
    sdk_version: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInstallationResponse {
    refresh_token: String,
    auth_token: GenerateAuthTokenResponse,
    fid: Option<String>,
}

#[derive(Serialize)]
struct GenerateAuthTokenRequest<'a> {
    installation: GenerateAuthTokenInstallation<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateAuthTokenInstallation<'a> {
    app_id: &'a str,
    sdk_version: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateAuthTokenResponse {
    token: String,
    expires_in: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    code: i64,
    message: String,
    status: String,
}

fn convert_auth_token(
    response: GenerateAuthTokenResponse,
) -> InstallationsResult<InstallationToken> {
    let expires_at = SystemTime::now() + parse_expires_in(&response.expires_in)?;
    Ok(InstallationToken {
        token: response.token,
        expires_at,
    })
}

fn parse_expires_in(raw: &str) -> InstallationsResult<Duration> {
    let stripped = raw
        .strip_suffix('s')
        .ok_or_else(|| invalid_argument(format!("Invalid expiresIn format: {}", raw)))?;
    let seconds: u64 = stripped
        .parse()
        .map_err(|err| invalid_argument(format!("Invalid expiresIn value '{}': {}", raw, err)))?;
    Ok(Duration::from_secs(seconds))
}

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::RestClient;

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
mod wasm;
#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
pub use wasm::RestClient;

#[cfg(all(target_arch = "wasm32", not(feature = "wasm-web")))]
compile_error!(
    "Building firebase-rs-sdk for wasm32 requires enabling the `wasm-web` feature to include the installations REST client."
);

#[cfg(test)]
mod tests;
