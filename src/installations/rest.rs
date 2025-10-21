use std::time::{Duration, SystemTime};

use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::installations::config::AppConfig;
use crate::installations::error::{
    internal_error, invalid_argument, request_failed, InstallationsError, InstallationsResult,
};
use crate::installations::types::InstallationToken;

const INSTALLATIONS_API_URL: &str = "https://firebaseinstallations.googleapis.com/v1";
const INTERNAL_AUTH_VERSION: &str = "FIS_v2";
const SDK_VERSION: &str = concat!("w:", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug)]
pub struct RestClient {
    http: Client,
    base_url: Url,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredInstallation {
    pub fid: String,
    pub refresh_token: String,
    pub auth_token: InstallationToken,
}

impl RestClient {
    pub fn new() -> InstallationsResult<Self> {
        let base_url = std::env::var("FIREBASE_INSTALLATIONS_API_URL")
            .unwrap_or_else(|_| INSTALLATIONS_API_URL.to_string());
        Self::with_base_url(&base_url)
    }

    pub fn with_base_url(base_url: &str) -> InstallationsResult<Self> {
        let base_url = Url::parse(base_url).map_err(|err| {
            invalid_argument(format!(
                "Invalid installations endpoint '{}': {}",
                base_url, err
            ))
        })?;

        let http = Client::builder()
            .user_agent(format!("firebase-rs-sdk/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| internal_error(format!("Failed to build HTTP client: {}", err)))?;

        Ok(Self { http, base_url })
    }

    pub fn register_installation(
        &self,
        config: &AppConfig,
        fid: &str,
    ) -> InstallationsResult<RegisteredInstallation> {
        let url = self.installations_endpoint(config, None)?;
        let headers = self.base_headers(&config.api_key)?;
        let body = CreateInstallationRequest {
            fid,
            auth_version: INTERNAL_AUTH_VERSION,
            app_id: &config.app_id,
            sdk_version: SDK_VERSION,
        };

        let response = self
            .send_with_retry(|| {
                self.http
                    .post(url.clone())
                    .headers(headers.clone())
                    .json(&body)
                    .send()
            })
            .map_err(|err| {
                internal_error(format!("Network error creating installation: {}", err))
            })?;

        if response.status().is_success() {
            let parsed: CreateInstallationResponse = response
                .json()
                .map_err(|err| internal_error(format!("Invalid installation response: {}", err)))?;
            return Ok(RegisteredInstallation {
                fid: parsed.fid.unwrap_or_else(|| fid.to_owned()),
                refresh_token: parsed.refresh_token,
                auth_token: convert_auth_token(parsed.auth_token)?,
            });
        }

        Err(self.request_failed("Create Installation", response))
    }

    pub fn generate_auth_token(
        &self,
        config: &AppConfig,
        fid: &str,
        refresh_token: &str,
    ) -> InstallationsResult<InstallationToken> {
        let mut url = self.installations_endpoint(config, Some(fid))?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| internal_error("Installations endpoint is not base"))?;
            segments.push("authTokens:generate");
        }

        let mut headers = self.base_headers(&config.api_key)?;
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("{} {}", INTERNAL_AUTH_VERSION, refresh_token))
                .map_err(|err| {
                    invalid_argument(format!("Invalid refresh token header: {}", err))
                })?,
        );

        let body = GenerateAuthTokenRequest {
            installation: GenerateAuthTokenInstallation {
                app_id: &config.app_id,
                sdk_version: SDK_VERSION,
            },
        };

        let response = self
            .send_with_retry(|| {
                self.http
                    .post(url.clone())
                    .headers(headers.clone())
                    .json(&body)
                    .send()
            })
            .map_err(|err| internal_error(format!("Network error refreshing token: {}", err)))?;

        if response.status().is_success() {
            let parsed: GenerateAuthTokenResponse = response
                .json()
                .map_err(|err| internal_error(format!("Invalid auth token response: {}", err)))?;
            return convert_auth_token(parsed);
        }

        Err(self.request_failed("Generate Auth Token", response))
    }

    fn installations_endpoint(
        &self,
        config: &AppConfig,
        fid: Option<&str>,
    ) -> InstallationsResult<Url> {
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| internal_error("Installations endpoint is not base"))?;
            segments.extend(["projects", config.project_id.as_str(), "installations"]);
            if let Some(fid) = fid {
                segments.push(fid);
            }
        }
        Ok(url)
    }

    fn base_headers(&self, api_key: &str) -> InstallationsResult<HeaderMap> {
        let mut headers = HeaderMap::with_capacity(3);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            HeaderName::from_static("x-goog-api-key"),
            HeaderValue::from_str(api_key).map_err(|err| {
                invalid_argument(format!("Invalid API key header value: {}", err))
            })?,
        );
        Ok(headers)
    }

    fn send_with_retry<F>(&self, mut send: F) -> Result<Response, reqwest::Error>
    where
        F: FnMut() -> Result<Response, reqwest::Error>,
    {
        let mut response = send()?;
        if response.status().is_server_error() {
            response = send()?;
        }
        Ok(response)
    }

    fn request_failed(&self, request_name: &str, response: Response) -> InstallationsError {
        let status = response.status();
        match response.json::<ErrorResponse>() {
            Ok(body) => request_failed(format!(
                "{} request failed with error \"{} {}: {}\"",
                request_name, body.error.code, body.error.status, body.error.message
            )),
            Err(err) => request_failed(format!(
                "{} request failed with status {} and unreadable body: {}",
                request_name, status, err
            )),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installations::config::AppConfig;
    use crate::installations::error::InstallationsErrorCode;
    use httpmock::prelude::*;
    use serde_json::json;
    use std::panic::{self, AssertUnwindSafe};

    fn test_config() -> AppConfig {
        AppConfig {
            app_name: "test".into(),
            api_key: "key".into(),
            project_id: "project".into(),
            app_id: "app".into(),
        }
    }

    fn try_start_server() -> Option<MockServer> {
        panic::catch_unwind(AssertUnwindSafe(|| MockServer::start())).ok()
    }

    #[test]
    fn register_installation_success() {
        let Some(server) = try_start_server() else {
            eprintln!("Skipping register_installation_success: unable to start mock server");
            return;
        };
        let _mock = server.mock(|when, then| {
            when.method(POST)
                .path("/projects/project/installations")
                .header("x-goog-api-key", "key");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "refreshToken": "refresh",
                    "authToken": {
                        "token": "token",
                        "expiresIn": "3600s"
                    },
                    "fid": "fid"
                }));
        });

        let client = RestClient::with_base_url(&server.base_url()).unwrap();
        let result = client
            .register_installation(&test_config(), "local-fid")
            .unwrap();

        assert_eq!(result.fid, "fid");
        assert_eq!(result.refresh_token, "refresh");
        assert_eq!(result.auth_token.token, "token");
    }

    #[test]
    fn generate_auth_token_success() {
        let Some(server) = try_start_server() else {
            eprintln!("Skipping generate_auth_token_success: unable to start mock server");
            return;
        };
        let _mock = server.mock(|when, then| {
            when.method(POST)
                .path("/projects/project/installations/fid/authTokens:generate")
                .header("x-goog-api-key", "key")
                .header(
                    "authorization",
                    format!("{} {}", INTERNAL_AUTH_VERSION, "refresh"),
                );
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "token": "token",
                    "expiresIn": "7200s"
                }));
        });

        let client = RestClient::with_base_url(&server.base_url()).unwrap();
        let token = client
            .generate_auth_token(&test_config(), "fid", "refresh")
            .unwrap();

        assert_eq!(token.token, "token");
    }

    #[test]
    fn parse_expires_in_rejects_invalid_format() {
        let err = parse_expires_in("1000ms").unwrap_err();
        assert!(matches!(err.code, InstallationsErrorCode::InvalidArgument));
    }
}
