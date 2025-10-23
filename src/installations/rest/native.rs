use std::future::Future;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, Response, Url};

use crate::installations::config::AppConfig;
use crate::installations::error::{
    internal_error, invalid_argument, request_failed as request_failed_err, InstallationsError,
    InstallationsResult,
};

use super::{
    convert_auth_token, CreateInstallationRequest, GenerateAuthTokenInstallation,
    GenerateAuthTokenRequest, RegisteredInstallation, INSTALLATIONS_API_URL, INTERNAL_AUTH_VERSION,
    SDK_VERSION,
};

#[derive(Clone, Debug)]
pub struct RestClient {
    http: Client,
    base_url: Url,
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

    pub async fn register_installation(
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
            .await
            .map_err(|err| {
                internal_error(format!("Network error creating installation: {}", err))
            })?;

        if response.status().is_success() {
            let parsed = response
                .json::<super::CreateInstallationResponse>()
                .await
                .map_err(|err| internal_error(format!("Invalid installation response: {}", err)))?;
            return Ok(RegisteredInstallation {
                fid: parsed.fid.unwrap_or_else(|| fid.to_owned()),
                refresh_token: parsed.refresh_token,
                auth_token: convert_auth_token(parsed.auth_token)?,
            });
        }

        Err(self.request_failed("Create Installation", response).await)
    }

    pub async fn generate_auth_token(
        &self,
        config: &AppConfig,
        fid: &str,
        refresh_token: &str,
    ) -> InstallationsResult<crate::installations::types::InstallationToken> {
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
            .await
            .map_err(|err| internal_error(format!("Network error refreshing token: {}", err)))?;

        if response.status().is_success() {
            let parsed = response
                .json::<super::GenerateAuthTokenResponse>()
                .await
                .map_err(|err| internal_error(format!("Invalid auth token response: {}", err)))?;
            return super::convert_auth_token(parsed);
        }

        Err(self.request_failed("Generate Auth Token", response).await)
    }

    pub async fn delete_installation(
        &self,
        config: &AppConfig,
        fid: &str,
        refresh_token: &str,
    ) -> InstallationsResult<()> {
        let url = self.installations_endpoint(config, Some(fid))?;
        let mut headers = self.base_headers(&config.api_key)?;
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("{} {}", INTERNAL_AUTH_VERSION, refresh_token))
                .map_err(|err| {
                    invalid_argument(format!("Invalid refresh token header: {}", err))
                })?,
        );

        let response = self
            .send_with_retry(|| {
                self.http
                    .delete(url.clone())
                    .headers(headers.clone())
                    .send()
            })
            .await
            .map_err(|err| {
                internal_error(format!("Network error deleting installation: {}", err))
            })?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(self.request_failed("Delete Installation", response).await)
        }
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

    async fn send_with_retry<F, Fut>(&self, mut send: F) -> Result<Response, reqwest::Error>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<Response, reqwest::Error>>,
    {
        let mut response = send().await?;
        if response.status().is_server_error() {
            response = send().await?;
        }
        Ok(response)
    }

    async fn request_failed(&self, request_name: &str, response: Response) -> InstallationsError {
        let status = response.status();
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => {
                return request_failed_err(format!(
                    "{} request failed with status {} and unreadable body: {}",
                    request_name, status, err
                ));
            }
        };

        match serde_json::from_slice::<super::ErrorResponse>(&bytes) {
            Ok(body) => request_failed_err(format!(
                "{} request failed with error \"{} {}: {}\"",
                request_name, body.error.code, body.error.status, body.error.message
            )),
            Err(err) => {
                let snippet = match String::from_utf8(bytes.to_vec()) {
                    Ok(text) => text,
                    Err(_) => "<binary>".to_string(),
                };
                request_failed_err(format!(
                    "{} request failed with status {} and unreadable body: {}; body: {}",
                    request_name, status, err, snippet
                ))
            }
        }
    }
}
