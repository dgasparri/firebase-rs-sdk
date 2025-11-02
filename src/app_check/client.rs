use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex};

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::json;

use crate::app::{FirebaseApp, HeartbeatService};
use crate::app_check::errors::{AppCheckError, AppCheckResult};
use crate::app_check::types::AppCheckToken;
use crate::app_check::util::parse_protobuf_duration;

const BASE_ENDPOINT: &str = "https://content-firebaseappcheck.googleapis.com/v1";
const EXCHANGE_RECAPTCHA_V3_METHOD: &str = "exchangeRecaptchaV3Token";
const EXCHANGE_RECAPTCHA_ENTERPRISE_METHOD: &str = "exchangeRecaptchaEnterpriseToken";

type ExchangeFuture = Pin<Box<dyn Future<Output = AppCheckResult<AppCheckToken>> + Send + 'static>>;

type ExchangeHandler =
    Arc<dyn Fn(ExchangeRequest, Option<Arc<dyn HeartbeatService>>) -> ExchangeFuture + Send + Sync>;

static EXCHANGE_OVERRIDE: LazyLock<Mutex<Option<ExchangeHandler>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Clone, Debug)]
pub struct ExchangeRequest {
    pub url: String,
    pub body: serde_json::Value,
}

#[derive(Deserialize)]
struct AppCheckResponse {
    token: String,
    ttl: String,
}

pub async fn exchange_token(
    request: ExchangeRequest,
    heartbeat: Option<Arc<dyn HeartbeatService>>,
) -> AppCheckResult<AppCheckToken> {
    let handler = EXCHANGE_OVERRIDE.lock().unwrap().clone();
    if let Some(handler) = handler {
        return handler(request, heartbeat).await;
    }

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    if let Some(service) = heartbeat {
        if let Some(header) =
            service
                .heartbeats_header()
                .await
                .map_err(|err| AppCheckError::FetchNetworkError {
                    message: err.to_string(),
                })?
        {
            headers.insert(
                "X-Firebase-Client",
                HeaderValue::from_str(&header).map_err(|err| AppCheckError::FetchNetworkError {
                    message: format!("invalid heartbeat header: {err}"),
                })?,
            );
        }
    }

    let client = reqwest::Client::new();
    let response = client
        .post(&request.url)
        .headers(headers)
        .json(&request.body)
        .send()
        .await
        .map_err(|err| AppCheckError::FetchNetworkError {
            message: err.to_string(),
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppCheckError::FetchStatusError {
            http_status: status.as_u16(),
        });
    }

    let body: AppCheckResponse =
        response
            .json()
            .await
            .map_err(|err| AppCheckError::FetchParseError {
                message: err.to_string(),
            })?;

    let ttl = parse_protobuf_duration(&body.ttl)?;
    AppCheckToken::with_ttl(body.token, ttl)
}

pub fn get_exchange_recaptcha_v3_request(
    app: &FirebaseApp,
    recaptcha_token: String,
) -> AppCheckResult<ExchangeRequest> {
    build_exchange_request(
        app,
        EXCHANGE_RECAPTCHA_V3_METHOD,
        "recaptcha_v3_token",
        recaptcha_token,
    )
}

pub fn get_exchange_recaptcha_enterprise_request(
    app: &FirebaseApp,
    recaptcha_token: String,
) -> AppCheckResult<ExchangeRequest> {
    build_exchange_request(
        app,
        EXCHANGE_RECAPTCHA_ENTERPRISE_METHOD,
        "recaptcha_enterprise_token",
        recaptcha_token,
    )
}

fn build_exchange_request(
    app: &FirebaseApp,
    method: &str,
    field: &str,
    token: String,
) -> AppCheckResult<ExchangeRequest> {
    let options = app.options();
    let project_id = options
        .project_id
        .ok_or_else(|| AppCheckError::InvalidConfiguration {
            message: "Firebase options must include project_id for App Check".into(),
        })?;
    let app_id = options
        .app_id
        .ok_or_else(|| AppCheckError::InvalidConfiguration {
            message: "Firebase options must include app_id for App Check".into(),
        })?;
    let api_key = options
        .api_key
        .ok_or_else(|| AppCheckError::InvalidConfiguration {
            message: "Firebase options must include api_key for App Check".into(),
        })?;

    let url = format!("{BASE_ENDPOINT}/projects/{project_id}/apps/{app_id}:{method}?key={api_key}");
    let body = json!({ field: token });

    Ok(ExchangeRequest { url, body })
}

#[cfg(test)]
pub(crate) fn set_exchange_override<F, Fut>(override_fn: F)
where
    F: Fn(ExchangeRequest, Option<Arc<dyn HeartbeatService>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppCheckResult<AppCheckToken>> + Send + 'static,
{
    let handler: ExchangeHandler =
        Arc::new(move |request, heartbeat| Box::pin(override_fn(request, heartbeat)));
    *EXCHANGE_OVERRIDE.lock().unwrap() = Some(handler);
}

#[cfg(test)]
pub(crate) fn clear_exchange_override() {
    *EXCHANGE_OVERRIDE.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FirebaseApp, FirebaseAppConfig, FirebaseOptions};
    use crate::component::ComponentContainer;

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_missing_project_id() {
        let app = FirebaseApp::new(
            FirebaseOptions {
                api_key: Some("key".into()),
                app_id: Some("app".into()),
                ..Default::default()
            },
            FirebaseAppConfig::new("test", false),
            ComponentContainer::new("test"),
        );

        let result = build_exchange_request(&app, "method", "field", "token".into());
        assert!(matches!(
            result,
            Err(AppCheckError::InvalidConfiguration { .. })
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn parses_response() {
        let app = FirebaseApp::new(
            FirebaseOptions {
                api_key: Some("key".into()),
                app_id: Some("app".into()),
                project_id: Some("project".into()),
                ..Default::default()
            },
            FirebaseAppConfig::new("test", false),
            ComponentContainer::new("test"),
        );

        let request = get_exchange_recaptcha_v3_request(&app, "captcha".into()).unwrap();
        assert!(request.url.contains("exchangeRecaptchaV3Token"));
        assert_eq!(request.body["recaptcha_v3_token"], "captcha");
    }
}
