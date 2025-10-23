use reqwest::header::{HeaderMap as ReqHeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Response, Url};

use super::{
    build_body, build_headers, map_subscribe_response, map_update_response, FcmRegistrationRequest,
    FcmResponse, FcmUpdateRequest, FCM_API_URL,
};
use crate::messaging::error::{
    internal_error, token_subscribe_failed, token_unsubscribe_failed, token_update_failed,
    MessagingResult,
};

#[derive(Clone, Debug)]
pub struct FcmClient {
    http: Client,
    base_url: Url,
}

impl FcmClient {
    pub fn new() -> MessagingResult<Self> {
        let base = std::env::var("FIREBASE_MESSAGING_FCM_ENDPOINT")
            .unwrap_or_else(|_| FCM_API_URL.to_string());
        Self::with_base_url(&base)
    }

    pub fn with_base_url(base_url: &str) -> MessagingResult<Self> {
        let url = Url::parse(base_url)
            .map_err(|err| internal_error(format!("Invalid FCM endpoint '{base_url}': {err}")))?;
        let http = Client::builder()
            .user_agent(format!("firebase-rs-sdk/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| internal_error(format!("Failed to build HTTP client: {err}")))?;
        Ok(Self {
            http,
            base_url: url,
        })
    }

    pub async fn register_token(
        &self,
        request: &FcmRegistrationRequest<'_>,
    ) -> MessagingResult<String> {
        let url = self.registration_endpoint(request.project_id)?;
        let headers = header_map(build_headers(
            request.api_key,
            request.installation_auth_token,
        )?)?;
        let body = build_body(&request.subscription);

        let response = self
            .http
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|err| token_subscribe_failed(err.to_string()))?;

        map_subscribe_response(self.parse_response(response).await?)
    }

    pub async fn update_token(&self, request: &FcmUpdateRequest<'_>) -> MessagingResult<String> {
        let url = self.registration_instance_endpoint(
            request.registration.project_id,
            request.registration_token,
        )?;
        let headers = header_map(build_headers(
            request.registration.api_key,
            request.registration.installation_auth_token,
        )?)?;
        let body = build_body(&request.registration.subscription);

        let response = self
            .http
            .patch(url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|err| token_update_failed(err.to_string()))?;

        map_update_response(self.parse_response(response).await?)
    }

    pub async fn delete_token(
        &self,
        project_id: &str,
        api_key: &str,
        installation_auth: &str,
        registration_token: &str,
    ) -> MessagingResult<()> {
        let url = self.registration_instance_endpoint(project_id, registration_token)?;
        let headers = header_map(build_headers(api_key, installation_auth)?)?;

        let response = self
            .http
            .delete(url)
            .headers(headers)
            .send()
            .await
            .map_err(|err| token_unsubscribe_failed(err.to_string()))?;

        let parsed = self.parse_response(response).await?;
        if let Some(error) = parsed.error {
            return Err(token_unsubscribe_failed(error.message));
        }
        Ok(())
    }

    fn registration_endpoint(&self, project_id: &str) -> MessagingResult<Url> {
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| internal_error("FCM endpoint is not base"))?;
            segments.extend(["projects", project_id, "registrations"]);
        }
        Ok(url)
    }

    fn registration_instance_endpoint(
        &self,
        project_id: &str,
        registration_token: &str,
    ) -> MessagingResult<Url> {
        let mut url = self.registration_endpoint(project_id)?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| internal_error("FCM endpoint is not base"))?;
            segments.push(registration_token);
        }
        Ok(url)
    }

    async fn parse_response(&self, response: Response) -> MessagingResult<FcmResponse> {
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|err| internal_error(format!("Failed to read FCM response: {err}")))?;
        serde_json::from_slice::<FcmResponse>(&bytes).map_err(|err| {
            internal_error(format!(
                "Failed to parse FCM response (status {status}): {err}"
            ))
        })
    }
}

fn header_map(headers: Vec<(String, String)>) -> MessagingResult<ReqHeaderMap> {
    let mut map = ReqHeaderMap::new();
    for (name, value) in headers {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|err| internal_error(format!("Invalid header name: {err}")))?;
        let header_value = HeaderValue::from_str(&value)
            .map_err(|err| internal_error(format!("Invalid header value: {err}")))?;
        map.append(header_name, header_value);
    }
    Ok(map)
}
