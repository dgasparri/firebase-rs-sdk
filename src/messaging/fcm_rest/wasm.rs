use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

use super::{
    backoff_delay_ms, build_body, build_headers, is_retriable_status, map_subscribe_response,
    map_update_response, FcmRegistrationRequest, FcmResponse, FcmUpdateRequest, FCM_API_URL,
};
use crate::messaging::constants::FCM_MAX_RETRIES;
use crate::messaging::error::{
    internal_error, token_subscribe_failed, token_unsubscribe_failed, token_update_failed,
    MessagingResult,
};

#[derive(Clone, Debug)]
pub struct FcmClient {
    base_url: String,
}

impl FcmClient {
    pub fn new() -> MessagingResult<Self> {
        Ok(Self {
            base_url: FCM_API_URL.to_string(),
        })
    }

    pub async fn register_token(
        &self,
        request: &FcmRegistrationRequest<'_>,
    ) -> MessagingResult<String> {
        let url = self.registration_endpoint(request.project_id);
        let headers = build_headers(request.api_key, request.installation_auth_token)?;
        let body = serde_json::to_string(&build_body(&request.subscription))
            .map_err(|err| internal_error(format!("Failed to encode FCM request: {err}")))?;

        let mut attempt = 0u32;

        loop {
            match self
                .send_request(url.clone(), "POST", headers.clone(), Some(body.clone()))
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    let parsed = self.parse_response(response).await?;

                    if status.is_success() {
                        return map_subscribe_response(parsed);
                    }

                    if is_retriable_status(status as u16) && attempt < FCM_MAX_RETRIES {
                        let delay = backoff_delay_ms(attempt);
                        attempt += 1;
                        super::sleep_ms(delay).await;
                        continue;
                    }

                    return map_subscribe_response(parsed);
                }
                Err(err) => {
                    if attempt >= FCM_MAX_RETRIES {
                        return Err(token_subscribe_failed(err));
                    }
                    let delay = backoff_delay_ms(attempt);
                    attempt += 1;
                    super::sleep_ms(delay).await;
                }
            }
        }
    }

    pub async fn update_token(&self, request: &FcmUpdateRequest<'_>) -> MessagingResult<String> {
        let url = self.registration_instance_endpoint(
            request.registration.project_id,
            request.registration_token,
        );
        let headers = build_headers(
            request.registration.api_key,
            request.registration.installation_auth_token,
        )?;
        let body = serde_json::to_string(&build_body(&request.registration.subscription))
            .map_err(|err| internal_error(format!("Failed to encode FCM request: {err}")))?;

        let mut attempt = 0u32;

        loop {
            match self
                .send_request(url.clone(), "PATCH", headers.clone(), Some(body.clone()))
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    let parsed = self.parse_response(response).await?;

                    if status.is_success() {
                        return map_update_response(parsed);
                    }

                    if is_retriable_status(status as u16) && attempt < FCM_MAX_RETRIES {
                        let delay = backoff_delay_ms(attempt);
                        attempt += 1;
                        super::sleep_ms(delay).await;
                        continue;
                    }

                    return map_update_response(parsed);
                }
                Err(err) => {
                    if attempt >= FCM_MAX_RETRIES {
                        return Err(token_update_failed(err));
                    }
                    let delay = backoff_delay_ms(attempt);
                    attempt += 1;
                    super::sleep_ms(delay).await;
                }
            }
        }
    }

    pub async fn delete_token(
        &self,
        project_id: &str,
        api_key: &str,
        installation_auth: &str,
        registration_token: &str,
    ) -> MessagingResult<()> {
        let url = self.registration_instance_endpoint(project_id, registration_token);
        let headers = build_headers(api_key, installation_auth)?;
        let mut attempt = 0u32;

        loop {
            match self
                .send_request(url.clone(), "DELETE", headers.clone(), None)
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    let parsed = self.parse_response(response).await?;

                    if status.is_success() {
                        if let Some(error) = parsed.error {
                            return Err(token_unsubscribe_failed(error.message));
                        }
                        return Ok(());
                    }

                    if is_retriable_status(status as u16) && attempt < FCM_MAX_RETRIES {
                        let delay = backoff_delay_ms(attempt);
                        attempt += 1;
                        super::sleep_ms(delay).await;
                        continue;
                    }

                    return Err(parsed
                        .error
                        .map(|err| token_unsubscribe_failed(err.message))
                        .unwrap_or_else(|| {
                            token_unsubscribe_failed(format!(
                                "FCM delete failed with status {status}"
                            ))
                        }));
                }
                Err(err) => {
                    if attempt >= FCM_MAX_RETRIES {
                        return Err(token_unsubscribe_failed(err));
                    }
                    let delay = backoff_delay_ms(attempt);
                    attempt += 1;
                    super::sleep_ms(delay).await;
                }
            }
        }
    }

    fn registration_endpoint(&self, project_id: &str) -> String {
        format!(
            "{}/projects/{}/registrations",
            self.base_url.trim_end_matches('/'),
            project_id
        )
    }

    fn registration_instance_endpoint(&self, project_id: &str, token: &str) -> String {
        format!("{}/{}", self.registration_endpoint(project_id), token)
    }

    async fn send_request(
        &self,
        url: String,
        method: &str,
        headers: Vec<(String, String)>,
        body: Option<String>,
    ) -> Result<Response, String> {
        let window = web_sys::window().ok_or_else(|| "Global window missing".to_string())?;
        let mut init = RequestInit::new();
        init.method(method);
        init.mode(RequestMode::Cors);
        if let Some(body) = body {
            init.body(Some(&JsValue::from_str(&body)));
        }

        let request =
            Request::new_with_str_and_init(&url, &init).map_err(|err| js_value_to_string(err))?;
        let request_headers = request.headers();
        for (name, value) in headers {
            request_headers
                .set(&name, &value)
                .map_err(|err| js_value_to_string(err))?;
        }

        let response = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|err| js_value_to_string(err))?;
        response
            .dyn_into::<Response>()
            .map_err(|err| js_value_to_string(err))
    }

    async fn parse_response(&self, response: Response) -> MessagingResult<FcmResponse> {
        let promise = response
            .text()
            .map_err(|err| internal_error(js_value_to_string(err)))?;
        let value = JsFuture::from(promise)
            .await
            .map_err(|err| internal_error(js_value_to_string(err)))?;
        let text = value
            .as_string()
            .ok_or_else(|| internal_error("FCM response body was not a string"))?;
        serde_json::from_str(&text)
            .map_err(|err| internal_error(format!("Failed to parse FCM response: {err}")))
    }
}

fn js_value_to_string(value: JsValue) -> String {
    if let Some(s) = value.as_string() {
        s
    } else if let Some(err) = value.dyn_ref::<web_sys::DomException>() {
        format!("{}: {}", err.name(), err.message())
    } else {
        format!("{:?}", value)
    }
}
