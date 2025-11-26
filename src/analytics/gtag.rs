//! Minimal gtag bootstrapper used to mirror the Firebase Analytics JS SDK's initialization flow.
//!
//! The Rust port does not attempt to inject script tags automatically in non-wasm targets, but
//! the helper keeps track of the desired data layer and configuration so WASM consumers can hook
//! into the same lifecycle.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use std::sync::LazyLock;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GtagState {
    pub data_layer_name: String,
    pub measurement_id: Option<String>,
    pub consent_settings: Option<BTreeMap<String, String>>,
    pub default_event_parameters: BTreeMap<String, String>,
    pub config: BTreeMap<String, String>,
    pub send_page_view: Option<bool>,
}

#[derive(Debug, Default)]
pub struct GtagRegistry {
    state: Mutex<GtagState>,
}

impl GtagRegistry {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(GtagState {
                data_layer_name: "dataLayer".to_string(),
                ..Default::default()
            }),
        }
    }

    pub fn set_data_layer_name(&self, data_layer: impl Into<String>) {
        self.state.lock().unwrap().data_layer_name = data_layer.into();
    }

    pub fn set_measurement_id(&self, measurement_id: Option<String>) {
        self.state.lock().unwrap().measurement_id = measurement_id;
    }

    pub fn set_consent_defaults(&self, consent: Option<BTreeMap<String, String>>) {
        self.state.lock().unwrap().consent_settings = consent;
    }

    pub fn set_default_event_parameters(&self, params: BTreeMap<String, String>) {
        self.state.lock().unwrap().default_event_parameters = params;
    }

    pub fn set_config(&self, config: BTreeMap<String, String>) {
        self.state.lock().unwrap().config = config;
    }

    pub fn set_send_page_view(&self, value: Option<bool>) {
        self.state.lock().unwrap().send_page_view = value;
    }

    pub fn snapshot(&self) -> GtagState {
        self.state.lock().unwrap().clone()
    }

    pub fn reset(&self) {
        *self.state.lock().unwrap() = GtagState {
            data_layer_name: "dataLayer".to_string(),
            ..Default::default()
        };
    }
}

#[derive(Clone, Debug)]
pub struct GlobalGtagRegistry(Arc<GtagRegistry>);

impl GlobalGtagRegistry {
    pub fn shared() -> Self {
        static INSTANCE: LazyLock<Arc<GtagRegistry>> = LazyLock::new(|| Arc::new(GtagRegistry::new()));
        Self(INSTANCE.clone())
    }

    pub fn inner(&self) -> &GtagRegistry {
        &self.0
    }
}
