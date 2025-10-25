use js_sys::Error as JsError;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response, Window};

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
    base_url: url::Url,
}

impl RestClient {
    pub fn new() -> InstallationsResult<Self> {
        let base_url = std::env::var("FIREBASE_INSTALLATIONS_API_URL")
            .unwrap_or_else(|_| INSTALLATIONS_API_URL.to_string());
        Self::with_base_url(&base_url)
    }

    pub fn with_base_url(base_url: &str) -> InstallationsResult<Self> {
        let base_url = url::Url::parse(base_url).map_err(|err| {
            invalid_argument(format!(
                "Invalid installations endpoint '{}': {}",
                base_url, err
            ))
        })?;
        Ok(Self { base_url })
    }

    pub async fn register_installation(
        &self,
        config: &AppConfig,
        fid: &str,
    ) -> InstallationsResult<RegisteredInstallation> {
        let url = self.installations_endpoint(config, None)?;
        let payload = CreateInstallationRequest {
            fid,
            auth_version: INTERNAL_AUTH_VERSION,
            app_id: &config.app_id,
            sdk_version: SDK_VERSION,
        };
        let body = serde_json::to_string(&payload)
            .map_err(|err| internal_error(format!("Failed to encode request body: {}", err)))?;

        let headers = vec![
            ("content-type", "application/json".to_string()),
            ("accept", "application/json".to_string()),
            ("x-goog-api-key", config.api_key.clone()),
        ];

        let response = self
            .send_with_retry(&url, "POST", &headers, Some(body))
            .await?;

        if response.ok() {
            let parsed = self
                .parse_json::<super::CreateInstallationResponse>(&response)
                .await?;
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

        let payload = GenerateAuthTokenRequest {
            installation: GenerateAuthTokenInstallation {
                app_id: &config.app_id,
                sdk_version: SDK_VERSION,
            },
        };

        let body = serde_json::to_string(&payload)
            .map_err(|err| internal_error(format!("Failed to encode request body: {}", err)))?;

        let headers = vec![
            ("content-type", "application/json".to_string()),
            ("accept", "application/json".to_string()),
            ("x-goog-api-key", config.api_key.clone()),
            (
                "authorization",
                format!("{} {}", INTERNAL_AUTH_VERSION, refresh_token),
            ),
        ];

        let response = self
            .send_with_retry(&url, "POST", &headers, Some(body))
            .await?;

        if response.ok() {
            let parsed = self
                .parse_json::<super::GenerateAuthTokenResponse>(&response)
                .await?;
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
        let headers = vec![
            ("accept", "application/json".to_string()),
            ("x-goog-api-key", config.api_key.clone()),
            (
                "authorization",
                format!("{} {}", INTERNAL_AUTH_VERSION, refresh_token),
            ),
        ];

        let response = self.send_with_retry(&url, "DELETE", &headers, None).await?;

        if response.ok() {
            Ok(())
        } else {
            Err(self.request_failed("Delete Installation", response).await)
        }
    }

    fn installations_endpoint(
        &self,
        config: &AppConfig,
        fid: Option<&str>,
    ) -> InstallationsResult<url::Url> {
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

    async fn send_with_retry(
        &self,
        url: &url::Url,
        method: &str,
        headers: &[(&str, String)],
        body: Option<String>,
    ) -> InstallationsResult<Response> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let response = self.dispatch(url, method, headers, body.clone()).await?;

            if response.status() >= 500 && attempt == 1 {
                continue;
            }

            return Ok(response);
        }
    }

    async fn dispatch(
        &self,
        url: &url::Url,
        method: &str,
        headers: &[(&str, String)],
        body: Option<String>,
    ) -> InstallationsResult<Response> {
        let window = window()?;
        let init = RequestInit::new();
        init.set_method(method);
        init.set_mode(RequestMode::Cors);
        let body_value = body.as_ref().map(|b| JsValue::from_str(b));
        if let Some(value) = body_value.as_ref() {
            init.set_body(value);
        }

        let request = Request::new_with_str_and_init(url.as_str(), &init)
            .map_err(|err| internal_error(js_value_to_string(err)))?;
        let request_headers = request.headers();
        for (name, value) in headers {
            request_headers
                .set(name, value)
                .map_err(|err| internal_error(js_value_to_string(err)))?;
        }

        let response = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|err| internal_error(js_value_to_string(err)))?;
        response
            .dyn_into::<Response>()
            .map_err(|err| internal_error(js_value_to_string(err)))
    }

    async fn parse_json<T>(&self, response: &Response) -> InstallationsResult<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let text = self.response_text(response).await?;
        serde_json::from_str(&text)
            .map_err(|err| internal_error(format!("Invalid response payload: {}", err)))
    }

    async fn response_text(&self, response: &Response) -> InstallationsResult<String> {
        let promise = response
            .text()
            .map_err(|err| internal_error(js_value_to_string(err)))?;
        let value = JsFuture::from(promise)
            .await
            .map_err(|err| internal_error(js_value_to_string(err)))?;
        value
            .as_string()
            .ok_or_else(|| internal_error("Response body was not a string"))
    }

    async fn request_failed(&self, request_name: &str, response: Response) -> InstallationsError {
        let status = response.status();
        match self.response_text(&response).await {
            Ok(body) => match serde_json::from_str::<super::ErrorResponse>(&body) {
                Ok(parsed) => request_failed_err(format!(
                    "{} request failed with error \"{} {}: {}\"",
                    request_name, parsed.error.code, parsed.error.status, parsed.error.message
                )),
                Err(err) => request_failed_err(format!(
                    "{} request failed with status {} and unreadable body: {}; body: {}",
                    request_name, status, err, body
                )),
            },
            Err(err) => request_failed_err(format!(
                "{} request failed with status {} and unreadable body: {}",
                request_name, status, err
            )),
        }
    }
}

fn window() -> InstallationsResult<Window> {
    web_sys::window().ok_or_else(|| internal_error("Global window object not available"))
}

fn js_value_to_string(value: JsValue) -> String {
    if let Some(s) = value.as_string() {
        s
    } else if let Some(err) = value.dyn_ref::<JsError>() {
        err.message()
            .as_string()
            .unwrap_or_else(|| "[object Error]".to_string())
    } else {
        format!("{:?}", value)
    }
}
