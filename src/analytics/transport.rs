use std::collections::BTreeMap;
#[cfg(target_arch = "wasm32")]
#[allow(unused_imports)]
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use reqwest::Client;
use reqwest::StatusCode;
use serde::Serialize;

use crate::analytics::error::{internal_error, invalid_argument, network_error, AnalyticsResult};

/// Configuration used to dispatch analytics events through the GA4 Measurement Protocol.
#[derive(Clone, Debug)]
pub struct MeasurementProtocolConfig {
    measurement_id: String,
    api_secret: String,
    endpoint: MeasurementProtocolEndpoint,
    timeout: Duration,
}

impl MeasurementProtocolConfig {
    pub fn new(measurement_id: impl Into<String>, api_secret: impl Into<String>) -> Self {
        Self {
            measurement_id: measurement_id.into(),
            api_secret: api_secret.into(),
            endpoint: MeasurementProtocolEndpoint::Collect,
            timeout: Duration::from_secs(10),
        }
    }

    pub fn with_endpoint(mut self, endpoint: MeasurementProtocolEndpoint) -> Self {
        self.endpoint = endpoint;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
    }

    pub(crate) fn measurement_id(&self) -> &str {
        &self.measurement_id
    }

    pub(crate) fn api_secret(&self) -> &str {
        &self.api_secret
    }
}

/// Supported endpoints for the Measurement Protocol.
#[derive(Clone, Debug)]
pub enum MeasurementProtocolEndpoint {
    /// Production collection endpoint: <https://www.google-analytics.com/mp/collect>
    Collect,
    /// Debugging endpoint: <https://www.google-analytics.com/debug/mp/collect>
    DebugCollect,
    /// Custom endpoint (primarily for testing).
    Custom(String),
}

impl MeasurementProtocolEndpoint {
    fn as_str(&self) -> &str {
        match self {
            MeasurementProtocolEndpoint::Collect => "https://www.google-analytics.com/mp/collect",
            MeasurementProtocolEndpoint::DebugCollect => {
                "https://www.google-analytics.com/debug/mp/collect"
            }
            MeasurementProtocolEndpoint::Custom(url) => url,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MeasurementProtocolDispatcher {
    client: Client,
    config: MeasurementProtocolConfig,
}

impl MeasurementProtocolDispatcher {
    /// Creates a new dispatcher that will send events to the GA4 Measurement Protocol endpoint.
    pub fn new(config: MeasurementProtocolConfig) -> AnalyticsResult<Self> {
        if config.measurement_id().trim().is_empty() {
            return Err(invalid_argument(
                "measurement protocol measurement_id must not be empty",
            ));
        }
        if config.api_secret().trim().is_empty() {
            return Err(invalid_argument(
                "measurement protocol api_secret must not be empty",
            ));
        }
        #[cfg(not(target_arch = "wasm32"))]
        let client = Client::builder()
            .timeout(config.timeout())
            .build()
            .map_err(|err| internal_error(format!("failed to build HTTP client: {err}")))?;

        #[cfg(target_arch = "wasm32")]
        let client = Client::builder()
            .build()
            .map_err(|err| internal_error(format!("failed to build HTTP client: {err}")))?;

        Ok(Self { client, config })
    }

    /// Sends a single analytics event via the measurement protocol.
    ///
    /// The caller is responsible for providing a stable `client_id`, typically sourced from
    /// Firebase Installations or another per-device identifier.
    pub async fn send_event(
        &self,
        client_id: &str,
        event_name: &str,
        params: &BTreeMap<String, String>,
    ) -> AnalyticsResult<()> {
        let payload = MeasurementPayloadOwned {
            client_id: client_id.to_owned(),
            events: vec![MeasurementEventOwned {
                name: event_name.to_owned(),
                params: params.clone(),
            }],
        };

        let body = serde_json::to_vec(&payload).map_err(|err| {
            internal_error(format!("failed to serialise analytics payload: {err}"))
        })?;

        let endpoint = self.config.endpoint.as_str().to_owned();
        let measurement_id = self.config.measurement_id().to_owned();
        let api_secret = self.config.api_secret().to_owned();
        let client = self.client.clone();

        let response = client
            .post(&endpoint)
            .query(&[
                ("measurement_id", measurement_id.as_str()),
                ("api_secret", api_secret.as_str()),
            ])
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|err| network_error(format!("failed to send analytics event: {err}")))?;

        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unavailable response body>".to_string());

        let message = match status {
            StatusCode::BAD_REQUEST => {
                format!("measurement protocol rejected the event (400). Response: {body}")
            }
            _ => format!(
                "measurement protocol request failed with status {status}. Response: {body}"
            ),
        };

        Err(network_error(message))
    }

    pub fn config(&self) -> &MeasurementProtocolConfig {
        &self.config
    }
}

#[derive(Serialize)]
struct MeasurementPayloadOwned {
    client_id: String,
    events: Vec<MeasurementEventOwned>,
}

#[derive(Serialize)]
struct MeasurementEventOwned {
    name: String,
    #[serde(rename = "params")]
    params: BTreeMap<String, String>,
}
