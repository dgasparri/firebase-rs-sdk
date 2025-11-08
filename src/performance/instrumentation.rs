use crate::performance::api::Performance;

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
mod wasm {
    use std::collections::HashMap;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use web_sys::{
        PerformanceNavigationTiming, PerformanceObserver, PerformanceObserverEntryList,
        PerformanceObserverInit, PerformanceResourceTiming,
    };

    use crate::performance::api::Performance;
    use crate::performance::error::PerformanceResult;
    use crate::platform::runtime;

    const PAGE_LOAD_TRACE: &str = "_wt_page_load";

    pub fn initialize(performance: &Performance) {
        if !performance.instrumentation_enabled() {
            return;
        }
        if let Err(err) = record_navigation_trace(performance) {
            log::debug!("navigation trace instrumentation failed: {err}");
        }
        if let Err(err) = observe_resources(performance.clone()) {
            log::debug!("resource observer setup failed: {err}");
        }
    }

    fn record_navigation_trace(performance: &Performance) -> PerformanceResult<()> {
        let window = web_sys::window().ok_or_else(|| internal("window unavailable"))?;
        let dom_perf = window
            .performance()
            .ok_or_else(|| internal("performance API unavailable"))?;
        let entries = dom_perf.get_entries_by_type("navigation");
        if entries.length() == 0 {
            return Ok(());
        }
        let entry = entries
            .get(0)
            .dyn_into::<PerformanceNavigationTiming>()
            .map_err(|_| internal("navigation entry cast failed"))?;
        let start_time_us =
            ((dom_perf.time_origin() + entry.start_time()) * 1000.0).max(0.0) as u128;
        let duration_us = (entry.duration() * 1000.0) as u64;
        let duration = std::time::Duration::from_micros(duration_us);
        let mut metrics = HashMap::new();
        let first_byte = entry.response_start() - entry.start_time();
        if first_byte > 0.0 {
            metrics.insert("_fcp".to_string(), first_byte as i64);
        }
        let perf_clone = performance.clone();
        runtime::spawn_detached(async move {
            if let Err(err) = perf_clone
                .record_auto_trace(
                    PAGE_LOAD_TRACE,
                    start_time_us,
                    duration,
                    metrics,
                    HashMap::new(),
                )
                .await
            {
                log::debug!("failed to record auto trace: {}", err);
            }
        });
        Ok(())
    }

    fn observe_resources(performance: Performance) -> PerformanceResult<()> {
        let callback = Closure::wrap(Box::new(
            move |list: PerformanceObserverEntryList, _: JsValue| {
                if !performance.instrumentation_enabled() {
                    return;
                }
                let entries = list.get_entries();
                for idx in 0..entries.length() {
                    let entry = entries.get(idx);
                    if entry.is_undefined() || entry.is_null() {
                        continue;
                    }
                    if let Ok(resource) = entry.dyn_into::<PerformanceResourceTiming>() {
                        if let Some(record) = build_network_record(&resource) {
                            let perf_clone = performance.clone();
                            runtime::spawn_detached(async move {
                                if let Err(err) = perf_clone.record_auto_network(record).await {
                                    log::debug!("failed to record auto network trace: {}", err);
                                }
                            });
                        }
                    }
                }
            },
        )
            as Box<dyn FnMut(PerformanceObserverEntryList, JsValue)>);

        let observer = PerformanceObserver::new(callback.as_ref().unchecked_ref())
            .map_err(|err| internal(&format!("observer init: {err:?}")))?;
        let mut types = js_sys::Array::new();
        types.push(&JsValue::from_str("resource"));
        let types_value: JsValue = types.into();
        let init = PerformanceObserverInit::new(&types_value);
        observer.observe(&init);
        callback.forget();
        Ok(())
    }

    fn build_network_record(
        resource: &PerformanceResourceTiming,
    ) -> Option<crate::performance::api::NetworkRequestRecord> {
        let url = resource.name();
        if url.starts_with("data:") || url.is_empty() {
            return None;
        }
        let sanitized_url = url.split('?').next().unwrap_or(&url).to_string();
        let window = web_sys::window()?;
        let perf = window.performance()?;
        let start_time_us = ((perf.time_origin() + resource.start_time()) * 1000.0) as u128;
        let duration_us = (resource.duration() * 1000.0) as u128;
        let response_initiated_us = if resource.response_start() > 0.0 {
            Some(((resource.response_start() - resource.start_time()) * 1000.0) as u128)
        } else {
            None
        };
        Some(crate::performance::api::NetworkRequestRecord {
            url: sanitized_url,
            http_method: crate::performance::api::HttpMethod::Get,
            start_time_us,
            time_to_request_completed_us: duration_us,
            time_to_response_initiated_us: response_initiated_us,
            time_to_response_completed_us: Some(duration_us),
            request_payload_bytes: Some(resource.transfer_size() as u64),
            response_payload_bytes: Some(resource.encoded_body_size() as u64),
            response_code: None,
            response_content_type: Some(resource.initiator_type()),
            app_check_token: None,
        })
    }

    fn internal(message: &str) -> crate::performance::error::PerformanceError {
        crate::performance::error::internal_error(message)
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub fn initialize(performance: &Performance) {
    wasm::initialize(performance);
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn initialize(_performance: &Performance) {}
