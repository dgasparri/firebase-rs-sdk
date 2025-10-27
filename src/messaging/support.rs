//! Environment capability checks for Firebase Messaging.
//!
//! The JavaScript SDK exposes `isSupported()` so consumers can guard calls to
//! messaging APIs on browsers that implement the Notification and Push APIs.
//! This module mirrors the behaviour for WebAssembly builds and falls back to
//! `false` for native targets.

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use js_sys::Reflect;
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
use wasm_bindgen::{JsCast, JsValue};

/// Returns `true` when the current environment exposes the browser APIs that
/// Firebase Cloud Messaging requires.
///
/// Port of `packages/messaging/src/api/isSupported.ts` in the Firebase JS SDK.
///
/// # Examples
///
/// ```
/// use firebase_rs_sdk::messaging;
///
/// if messaging::is_supported() {
///     // Safe to call messaging APIs that rely on browser push features.
/// }
/// ```
#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
pub fn is_supported() -> bool {
    let window = match web_sys::window() {
        Some(window) => window,
        None => return false,
    };
    let navigator = window.navigator();
    let navigator_js = JsValue::from(navigator.clone());

    let cookie_enabled = Reflect::get(&navigator_js, &JsValue::from_str("cookieEnabled"))
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if !cookie_enabled {
        return false;
    }

    let service_worker_available = Reflect::get(&navigator_js, &JsValue::from_str("serviceWorker"))
        .ok()
        .map(|value| !value.is_undefined() && !value.is_null())
        .unwrap_or(false);
    if !service_worker_available {
        return false;
    }

    // Ensure indexedDB is available. We only check that the factory exists; the
    // JavaScript SDK further verifies openability, which we can add once async
    // event handling is wired up.
    match window.indexed_db() {
        Ok(Some(_)) => {}
        _ => return false,
    }

    let window_js = JsValue::from(window.clone());
    if !property_in(&window_js, "PushManager")
        || !property_in(&window_js, "Notification")
        || !property_in(&window_js, "fetch")
    {
        return false;
    }

    if !prototype_has_property(&window_js, "ServiceWorkerRegistration", "showNotification") {
        return false;
    }

    if !prototype_has_property(&window_js, "PushSubscription", "getKey") {
        return false;
    }

    true
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn property_in(target: &JsValue, property: &str) -> bool {
    Reflect::has(target, &JsValue::from_str(property)).unwrap_or(false)
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
fn prototype_has_property(target: &JsValue, constructor: &str, property: &str) -> bool {
    let ctor = match Reflect::get(target, &JsValue::from_str(constructor)) {
        Ok(value) => value,
        Err(_) => return false,
    };

    let prototype = match Reflect::get(&ctor, &JsValue::from_str("prototype")) {
        Ok(value) => value,
        Err(_) => return false,
    };

    prototype
        .dyn_ref::<js_sys::Object>()
        .map(|obj| obj.has_own_property(&JsValue::from_str(property)))
        .unwrap_or(false)
}

/// Returns `false` outside a web environment, where the required browser APIs
/// are unavailable.
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
pub fn is_supported() -> bool {
    false
}

#[cfg(all(test, not(all(feature = "wasm-web", target_arch = "wasm32"))))]
mod tests {
    #[test]
    fn non_wasm_targets_are_not_supported() {
        assert!(!super::is_supported());
    }
}
