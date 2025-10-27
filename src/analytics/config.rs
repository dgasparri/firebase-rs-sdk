#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use crate::analytics::error::{
    config_fetch_failed, internal_error, missing_measurement_id, AnalyticsResult,
};
use crate::app::FirebaseApp;

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

/// Fetches remote dynamic configuration for the provided Firebase app using the REST endpoint
/// `/v1alpha/projects/-/apps/{app_id}/webConfig`.
pub(crate) async fn fetch_dynamic_config(app: &FirebaseApp) -> AnalyticsResult<DynamicConfig> {
    let options = app.options();
    let app_id = options.app_id.clone().ok_or_else(|| {
        missing_measurement_id(
            "Firebase options are missing `app_id`; unable to fetch analytics configuration",
        )
    })?;

    let api_key = options.api_key.clone().ok_or_else(|| {
        missing_measurement_id(
            "Firebase options are missing `api_key`; unable to fetch analytics configuration",
        )
    })?;

    let url = dynamic_config_url(&app_id);

    #[cfg(not(target_arch = "wasm32"))]
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| internal_error(format!("failed to build HTTP client: {err}")))?;

    #[cfg(target_arch = "wasm32")]
    let client = Client::builder()
        .build()
        .map_err(|err| internal_error(format!("failed to build HTTP client: {err}")))?;

    let response = client
        .get(url)
        .header("x-goog-api-key", api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|err| config_fetch_failed(format!("failed to fetch analytics config: {err}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unavailable response body>".to_string());
        return Err(config_fetch_failed(format!(
            "analytics config request failed with status {status}: {body}"
        )));
    }

    let parsed: RemoteConfigResponse = response
        .json()
        .await
        .map_err(|err| config_fetch_failed(format!("invalid analytics config response: {err}")))?;

    let measurement_id = match parsed.measurement_id {
        Some(value) => value,
        None => {
            return Err(config_fetch_failed(
                "remote analytics config response did not include a measurement ID",
            ))
        }
    };

    Ok(DynamicConfig::new(measurement_id, parsed.app_id))
}

fn dynamic_config_url(app_id: &str) -> String {
    if let Ok(template) = std::env::var("FIREBASE_ANALYTICS_CONFIG_URL") {
        return template.replace("{app-id}", app_id);
    }

    format!(
        "https://firebase.googleapis.com/v1alpha/projects/-/apps/{}/webConfig",
        app_id
    )
}

#[derive(Deserialize)]
struct RemoteConfigResponse {
    #[serde(rename = "measurementId")]
    measurement_id: Option<String>,
    #[serde(rename = "appId")]
    app_id: Option<String>,
}
