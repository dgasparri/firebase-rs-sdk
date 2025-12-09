use std::collections::HashMap;
#[cfg(test)]
use std::collections::VecDeque;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Deserialize;

use crate::analytics::error::{config_fetch_failed, internal_error, missing_measurement_id, AnalyticsResult};
use crate::app::FirebaseApp;
use crate::platform::runtime::{sleep, with_timeout};

/// Minimal dynamic configuration returned by the Firebase Analytics config endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynamicConfig {
    measurement_id: String,
    app_id: Option<String>,
}

impl DynamicConfig {
    pub fn new(measurement_id: impl Into<String>, app_id: Option<String>) -> Self {
        Self {
            measurement_id: measurement_id.into(),
            app_id,
        }
    }

    pub fn measurement_id(&self) -> &str {
        &self.measurement_id
    }

    pub fn app_id(&self) -> Option<&str> {
        self.app_id.as_deref()
    }
}

/// Attempts to build a dynamic config directly from the locally supplied Firebase options.
pub(crate) fn from_app_options(app: &FirebaseApp) -> Option<DynamicConfig> {
    let options = app.options();
    options
        .measurement_id
        .as_ref()
        .map(|mid| DynamicConfig::new(mid.clone(), options.app_id.clone()))
}

/// Retry/backoff configuration used when fetching analytics dynamic config.
#[derive(Clone, Debug)]
pub(crate) struct FetchRetrySettings {
    pub fetch_timeout: Duration,
    pub base_retry_interval: Duration,
    pub retry_factor: u32,
    pub long_retry_factor: u32,
    pub max_attempts: Option<u32>,
}

impl Default for FetchRetrySettings {
    fn default() -> Self {
        Self {
            fetch_timeout: Duration::from_secs(60),
            base_retry_interval: Duration::from_millis(1_000),
            retry_factor: 2,
            long_retry_factor: 30,
            max_attempts: None,
        }
    }
}

#[derive(Clone, Debug)]
struct ThrottleMetadata {
    throttle_end_time: Instant,
    backoff_count: u32,
}

impl Default for ThrottleMetadata {
    fn default() -> Self {
        Self {
            throttle_end_time: Instant::now(),
            backoff_count: 0,
        }
    }
}

#[derive(Clone, Debug)]
struct FetchFailure {
    retriable: bool,
    status: Option<reqwest::StatusCode>,
    error: crate::analytics::error::AnalyticsError,
}

impl FetchFailure {
    fn non_retriable(error: crate::analytics::error::AnalyticsError, status: Option<reqwest::StatusCode>) -> Self {
        Self {
            retriable: false,
            status,
            error,
        }
    }

    fn retriable(error: crate::analytics::error::AnalyticsError, status: Option<reqwest::StatusCode>) -> Self {
        Self {
            retriable: true,
            status,
            error,
        }
    }
}

static RETRY_DATA: LazyLock<Mutex<HashMap<String, ThrottleMetadata>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(test)]
pub(crate) fn reset_retry_state() {
    RETRY_DATA.lock().unwrap().clear();
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct MockHttpResponse {
    pub status: u16,
    pub body: Option<String>,
    pub delay: Duration,
}

#[cfg(test)]
static MOCK_HTTP_RESPONSES: LazyLock<Mutex<HashMap<String, VecDeque<MockHttpResponse>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(test)]
pub(crate) fn reset_mock_http_responses() {
    MOCK_HTTP_RESPONSES.lock().unwrap().clear();
}

#[cfg(test)]
pub(crate) fn enqueue_mock_response(app_id: &str, response: MockHttpResponse) {
    let mut guard = MOCK_HTTP_RESPONSES.lock().unwrap();
    guard.entry(app_id.to_string()).or_default().push_back(response);
}

/// Fetches remote dynamic configuration for the provided Firebase app using the REST endpoint
/// `/v1alpha/projects/-/apps/{app_id}/webConfig`, retrying on transient errors and falling back to
/// the locally configured measurement ID when available.
pub(crate) async fn fetch_dynamic_config_with_retry(app: &FirebaseApp) -> AnalyticsResult<DynamicConfig> {
    fetch_dynamic_config_with_retry_internal(app, FetchRetrySettings::default()).await
}

#[cfg(test)]
pub(crate) async fn fetch_dynamic_config_with_settings(
    app: &FirebaseApp,
    settings: FetchRetrySettings,
) -> AnalyticsResult<DynamicConfig> {
    fetch_dynamic_config_with_retry_internal(app, settings).await
}

async fn fetch_dynamic_config_with_retry_internal(
    app: &FirebaseApp,
    settings: FetchRetrySettings,
) -> AnalyticsResult<DynamicConfig> {
    let local_config = from_app_options(app);
    let local_measurement_id = local_config.as_ref().map(|cfg| cfg.measurement_id().to_string());

    let options = app.options();
    let app_id = options.app_id.clone().ok_or_else(|| {
        missing_measurement_id("Firebase options are missing `app_id`; unable to fetch analytics configuration")
    })?;

    let api_key = options.api_key.clone();

    if api_key.is_none() {
        if let Some(mid) = local_measurement_id {
            log::warn!(
                "Firebase options are missing `api_key`; falling back to locally configured measurement ID `{mid}`"
            );
            return Ok(DynamicConfig::new(mid, Some(app_id)));
        }
        return Err(missing_measurement_id(
            "Firebase options are missing `api_key` and `measurement_id`; unable to fetch analytics configuration",
        ));
    }

    let api_key = api_key.expect("api_key checked above");
    let client = build_http_client(settings.fetch_timeout)?;

    let mut attempts: u32 = 0;
    loop {
        attempts = attempts.saturating_add(1);
        if let Some(limit) = settings.max_attempts {
            if attempts > limit {
                let message = "analytics config request exceeded retry limit";
                if let Some(mid) = local_measurement_id.clone() {
                    log::warn!("{message}; falling back to local measurement ID `{mid}`");
                    return Ok(DynamicConfig::new(mid, Some(app_id)));
                }
                return Err(config_fetch_failed(message));
            }
        }

        let throttle = {
            let guard = RETRY_DATA.lock().unwrap();
            guard.get(&app_id).cloned().unwrap_or_default()
        };

        let now = Instant::now();
        if throttle.throttle_end_time > now {
            sleep(throttle.throttle_end_time - now).await;
        }

        let attempt = with_timeout(try_fetch_config(&client, &app_id, &api_key), settings.fetch_timeout).await;

        match attempt {
            Ok(Ok(config)) => {
                RETRY_DATA.lock().unwrap().remove(&app_id);
                return Ok(config);
            }
            Ok(Err(failure)) => {
                if !failure.retriable {
                    RETRY_DATA.lock().unwrap().remove(&app_id);
                    if let Some(mid) = local_measurement_id.clone() {
                        log::warn!(
                            "Failed to fetch analytics config for app `{app_id}`: {}; falling back to local measurement ID `{mid}`",
                            failure.error
                        );
                        return Ok(DynamicConfig::new(mid, Some(app_id)));
                    }
                    return Err(failure.error);
                }

                let backoff = calculate_backoff(&settings, throttle.backoff_count, failure.status);
                let throttle_metadata = ThrottleMetadata {
                    throttle_end_time: Instant::now() + backoff,
                    backoff_count: throttle.backoff_count.saturating_add(1),
                };
                RETRY_DATA.lock().unwrap().insert(app_id.clone(), throttle_metadata);
            }
            Err(_) => {
                RETRY_DATA.lock().unwrap().remove(&app_id);
                if let Some(mid) = local_measurement_id.clone() {
                    log::warn!(
                        "Timed out fetching analytics config for app `{app_id}`; falling back to local measurement ID `{mid}`"
                    );
                    return Ok(DynamicConfig::new(mid, Some(app_id)));
                }

                return Err(config_fetch_failed(
                    "analytics config request timed out and no fallback measurement ID was available",
                ));
            }
        }
    }
}

fn calculate_backoff(
    settings: &FetchRetrySettings,
    backoff_count: u32,
    status: Option<reqwest::StatusCode>,
) -> Duration {
    let factor = match status {
        Some(reqwest::StatusCode::SERVICE_UNAVAILABLE) => settings.long_retry_factor,
        _ => settings.retry_factor,
    };

    let exponent = factor.saturating_pow(backoff_count.saturating_add(1));
    let base_ms = settings
        .base_retry_interval
        .as_millis()
        .saturating_mul(exponent as u128);
    Duration::from_millis(base_ms.min(u64::MAX as u128) as u64)
}

// underscore in the variable to prevent #warn unused_variable for non-wasm targets.
fn build_http_client(_timeout: Duration) -> AnalyticsResult<Client> {
    #[cfg(not(target_arch = "wasm32"))]
    let client = Client::builder()
        .timeout(_timeout)
        .build()
        .map_err(|err| internal_error(format!("failed to build HTTP client: {err}")))?;

    #[cfg(target_arch = "wasm32")]
    let client = Client::builder()
        .build()
        .map_err(|err| internal_error(format!("failed to build HTTP client: {err}")))?;

    Ok(client)
}

async fn try_fetch_config(client: &Client, app_id: &str, api_key: &str) -> Result<DynamicConfig, FetchFailure> {
    #[cfg(test)]
    if let Some(mock) = {
        let mut guard = MOCK_HTTP_RESPONSES.lock().unwrap();
        guard.get_mut(app_id).and_then(|queue| queue.pop_front())
    } {
        if !mock.delay.is_zero() {
            sleep(mock.delay).await;
        }
        let status = reqwest::StatusCode::from_u16(mock.status).unwrap_or(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        if !(status.is_success() || status == reqwest::StatusCode::NOT_MODIFIED) {
            let message = format!("analytics config request failed with status {status}: <mocked response>");
            let retriable = matches!(
                status,
                reqwest::StatusCode::TOO_MANY_REQUESTS
                    | reqwest::StatusCode::INTERNAL_SERVER_ERROR
                    | reqwest::StatusCode::BAD_GATEWAY
                    | reqwest::StatusCode::SERVICE_UNAVAILABLE
            );
            let failure = if retriable {
                FetchFailure::retriable(config_fetch_failed(message), Some(status))
            } else {
                FetchFailure::non_retriable(config_fetch_failed(message), Some(status))
            };
            return Err(failure);
        }

        let parsed_body = mock
            .body
            .unwrap_or_else(|| "{\"measurementId\":\"G-MOCK\",\"appId\":\"mock\"}".to_string());
        let parsed: RemoteConfigResponse = serde_json::from_str(&parsed_body).map_err(|err| {
            FetchFailure::non_retriable(
                config_fetch_failed(format!("invalid analytics config response: {err}")),
                Some(status),
            )
        })?;
        let measurement_id = parsed.measurement_id.ok_or_else(|| {
            FetchFailure::non_retriable(
                config_fetch_failed("remote analytics config response did not include a measurement ID"),
                Some(status),
            )
        })?;

        return Ok(DynamicConfig::new(measurement_id, parsed.app_id));
    }

    let url = dynamic_config_url(app_id);
    let response = client
        .get(url)
        .header("x-goog-api-key", api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|err| {
            FetchFailure::retriable(config_fetch_failed(format!("failed to fetch analytics config: {err}")), None)
        })?;

    let status = response.status();
    if !(status.is_success() || status == reqwest::StatusCode::NOT_MODIFIED) {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unavailable response body>".to_string());
        let message = format!("analytics config request failed with status {status}: {body}");
        let retriable = matches!(
            status,
            reqwest::StatusCode::TOO_MANY_REQUESTS
                | reqwest::StatusCode::INTERNAL_SERVER_ERROR
                | reqwest::StatusCode::BAD_GATEWAY
                | reqwest::StatusCode::SERVICE_UNAVAILABLE
        );
        let failure = if retriable {
            FetchFailure::retriable(config_fetch_failed(message), Some(status))
        } else {
            FetchFailure::non_retriable(config_fetch_failed(message), Some(status))
        };
        return Err(failure);
    }

    let parsed: RemoteConfigResponse = response.json().await.map_err(|err| {
        FetchFailure::non_retriable(
            config_fetch_failed(format!("invalid analytics config response: {err}")),
            Some(status),
        )
    })?;

    let measurement_id = parsed.measurement_id.ok_or_else(|| {
        FetchFailure::non_retriable(
            config_fetch_failed("remote analytics config response did not include a measurement ID"),
            Some(status),
        )
    })?;

    Ok(DynamicConfig::new(measurement_id, parsed.app_id))
}

fn dynamic_config_url(app_id: &str) -> String {
    if let Ok(template) = std::env::var("FIREBASE_ANALYTICS_CONFIG_URL") {
        return template.replace("{app-id}", app_id);
    }

    format!("https://firebase.googleapis.com/v1alpha/projects/-/apps/{}/webConfig", app_id)
}

#[derive(Deserialize)]
struct RemoteConfigResponse {
    #[serde(rename = "measurementId")]
    measurement_id: Option<String>,
    #[serde(rename = "appId")]
    app_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn unique_settings() -> FirebaseAppSettings {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!("analytics-config-{}", COUNTER.fetch_add(1, Ordering::SeqCst))),
            ..Default::default()
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn falls_back_to_local_measurement_id_when_api_key_missing() {
        reset_retry_state();
        reset_mock_http_responses();
        let options = FirebaseOptions {
            app_id: Some("1:123:web:abc".into()),
            measurement_id: Some("G-LOCALMID".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();

        let config = fetch_dynamic_config_with_retry(&app).await.unwrap();
        assert_eq!(config.measurement_id(), "G-LOCALMID");
        assert_eq!(config.app_id(), Some("1:123:web:abc"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn retries_on_transient_error_then_succeeds() {
        reset_retry_state();
        reset_mock_http_responses();
        enqueue_mock_response(
            "1:retry:web:abc",
            MockHttpResponse {
                status: 503,
                body: Some("{}".to_string()),
                delay: Duration::from_millis(0),
            },
        );
        enqueue_mock_response(
            "1:retry:web:abc",
            MockHttpResponse {
                status: 200,
                body: Some(r#"{"measurementId":"G-REMOTE123","appId":"1:retry:web:abc"}"#.to_string()),
                delay: Duration::from_millis(0),
            },
        );

        let options = FirebaseOptions {
            app_id: Some("1:retry:web:abc".into()),
            api_key: Some("api-key".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();

        let config = fetch_dynamic_config_with_settings(
            &app,
            FetchRetrySettings {
                fetch_timeout: Duration::from_millis(200),
                base_retry_interval: Duration::from_millis(10),
                retry_factor: 2,
                long_retry_factor: 3,
                max_attempts: Some(3),
            },
        )
        .await
        .unwrap();

        assert_eq!(config.measurement_id(), "G-REMOTE123");
        assert_eq!(config.app_id(), Some("1:retry:web:abc"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn times_out_and_falls_back_to_local_measurement_id() {
        reset_retry_state();
        reset_mock_http_responses();
        enqueue_mock_response(
            "1:timeout:web:abc",
            MockHttpResponse {
                status: 200,
                body: Some(r#"{"measurementId":"G-REMOTE999","appId":"1:timeout:web:abc"}"#.to_string()),
                delay: Duration::from_millis(200),
            },
        );

        let options = FirebaseOptions {
            app_id: Some("1:timeout:web:abc".into()),
            api_key: Some("api-key".into()),
            measurement_id: Some("G-FALLBACK".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).await.unwrap();

        let config = fetch_dynamic_config_with_settings(
            &app,
            FetchRetrySettings {
                fetch_timeout: Duration::from_millis(50),
                base_retry_interval: Duration::from_millis(5),
                retry_factor: 2,
                long_retry_factor: 2,
                max_attempts: Some(2),
            },
        )
        .await
        .unwrap();

        assert_eq!(config.measurement_id(), "G-FALLBACK");
        assert_eq!(config.app_id(), Some("1:timeout:web:abc"));
    }
}
