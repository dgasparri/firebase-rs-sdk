use std::env;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::Serialize;

use crate::performance::api::Performance;
use crate::performance::error::{internal_error, PerformanceResult};
use crate::performance::storage::{
    SerializableNetworkRequest, SerializableTrace, TraceEnvelope, TraceStoreHandle,
};
use crate::platform::runtime;
use chrono::Utc;

const DEFAULT_ENDPOINT: &str = "https://firebaselogging.googleapis.com/v0cc/log?format=json_proto3";
const DEFAULT_BATCH_SIZE: usize = 25;
const DEFAULT_INTERVAL: Duration = Duration::from_secs(10);
const INITIAL_DELAY: Duration = Duration::from_millis(2500);

#[derive(Clone, Debug)]
pub struct TransportOptions {
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub flush_interval: Option<Duration>,
    pub max_batch_size: Option<usize>,
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            endpoint: Some(DEFAULT_ENDPOINT.to_string()),
            api_key: None,
            flush_interval: None,
            max_batch_size: None,
        }
    }
}

pub struct TransportController {
    performance: Performance,
    store: TraceStoreHandle,
    options: Arc<RwLock<TransportOptions>>,
    client: Arc<HttpTransportClient>,
}

impl TransportController {
    pub fn new(
        performance: Performance,
        store: TraceStoreHandle,
        options: Arc<RwLock<TransportOptions>>,
    ) -> Arc<Self> {
        let controller = Arc::new(Self {
            performance,
            store,
            options,
            client: Arc::new(HttpTransportClient::default()),
        });
        controller.spawn();
        controller
    }

    fn spawn(self: &Arc<Self>) {
        let this = Arc::clone(self);
        runtime::spawn_detached(async move {
            runtime::sleep(INITIAL_DELAY).await;
            this.run().await;
        });
    }

    async fn run(self: Arc<Self>) {
        loop {
            runtime::sleep(self.current_interval()).await;
            if let Err(err) = self.flush_once().await {
                log::debug!("performance transport flush failed: {err}");
            }
        }
    }

    fn current_interval(&self) -> Duration {
        self.options
            .read()
            .map(|options| options.flush_interval.unwrap_or(DEFAULT_INTERVAL))
            .unwrap_or(DEFAULT_INTERVAL)
    }

    fn batch_size(&self) -> usize {
        self.options
            .read()
            .map(|options| options.max_batch_size.unwrap_or(DEFAULT_BATCH_SIZE))
            .unwrap_or(DEFAULT_BATCH_SIZE)
    }

    pub async fn flush_once(&self) -> PerformanceResult<()> {
        if !self.performance.data_collection_enabled() {
            return Ok(());
        }
        let endpoint = match self.current_endpoint() {
            Some(url) => url,
            None => return Ok(()),
        };
        let batch = self.store.drain(self.batch_size()).await?;
        if batch.is_empty() {
            return Ok(());
        }
        let payload = self.build_payload(&batch).await?;
        if let Err(err) = self.client.send(&endpoint, &payload).await {
            log::debug!("performance transport send failed: {err}");
            self.requeue(batch).await?;
        }
        Ok(())
    }

    async fn requeue(&self, entries: Vec<TraceEnvelope>) -> PerformanceResult<()> {
        for entry in entries {
            self.store.push(entry).await?;
        }
        Ok(())
    }

    pub fn trigger_flush(self: &Arc<Self>) {
        let controller = Arc::clone(self);
        runtime::spawn_detached(async move {
            if let Err(err) = controller.flush_once().await {
                log::debug!("performance transport ad-hoc flush failed: {err}");
            }
        });
    }

    fn current_endpoint(&self) -> Option<String> {
        if env::var("FIREBASE_PERF_DISABLE_TRANSPORT").is_ok() {
            return None;
        }
        self.options.read().ok().and_then(|options| {
            options.endpoint.as_ref().map(|base| {
                let mut url = base.clone();
                let key = options
                    .api_key
                    .clone()
                    .or_else(|| self.performance.app().options().api_key.clone());
                if let Some(key) = key {
                    if url.contains('?') {
                        url.push('&');
                    } else {
                        url.push('?');
                    }
                    url.push_str("key=");
                    url.push_str(&key);
                }
                url
            })
        })
    }

    async fn build_payload(&self, batch: &[TraceEnvelope]) -> PerformanceResult<TransportPayload> {
        let mut traces = Vec::new();
        let mut network = Vec::new();
        for entry in batch {
            match entry {
                TraceEnvelope::Trace(trace) => traces.push(SerializableTrace::from(trace)),
                TraceEnvelope::Network(record) => {
                    network.push(SerializableNetworkRequest::from(record))
                }
            }
        }
        Ok(TransportPayload {
            request_time_ms: format!("{}", Utc::now().timestamp_millis()),
            app_id: self.performance.app().options().app_id.clone(),
            project_id: self.performance.app().options().project_id.clone(),
            installation_id: self.performance.installation_id().await,
            platform: current_platform(),
            sdk_version: env!("CARGO_PKG_VERSION").to_string(),
            traces,
            network_requests: network,
        })
    }
}

fn current_platform() -> String {
    #[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
    {
        return "wasm".into();
    }
    #[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
    {
        return "native".into();
    }
}

struct HttpTransportClient {
    client: reqwest::Client,
}

impl Default for HttpTransportClient {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl HttpTransportClient {
    async fn send(&self, endpoint: &str, payload: &TransportPayload) -> PerformanceResult<()> {
        let response = self
            .client
            .post(endpoint)
            .json(payload)
            .send()
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        if !response.status().is_success() {
            return Err(internal_error(format!(
                "transport responded with status {}",
                response.status()
            )));
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct TransportPayload {
    request_time_ms: String,
    app_id: Option<String>,
    project_id: Option<String>,
    installation_id: Option<String>,
    platform: String,
    sdk_version: String,
    traces: Vec<SerializableTrace>,
    network_requests: Vec<SerializableNetworkRequest>,
}
