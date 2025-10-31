//! Shared runtime environment detection and default configuration helpers.

use std::env;
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use serde_json::{Map, Value};

/// Returns the parsed `__FIREBASE_DEFAULTS__` object when available.
fn firebase_defaults() -> Option<Value> {
    defaults_from_env()
        .or_else(defaults_from_path)
        .or_else(defaults_from_global)
}

fn defaults_from_env() -> Option<Value> {
    let raw = env::var("__FIREBASE_DEFAULTS__").ok()?;
    parse_json_value(raw)
}

fn defaults_from_path() -> Option<Value> {
    let path = env::var("__FIREBASE_DEFAULTS_PATH").ok()?;
    let content = fs::read_to_string(path).ok()?;
    parse_json_value(content)
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
fn defaults_from_global() -> Option<Value> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;

    let global = js_sys::global();
    let value = js_sys::Reflect::get(&global, &JsValue::from_str("__FIREBASE_DEFAULTS__")).ok()?;
    if value.is_null() || value.is_undefined() {
        return None;
    }
    let serialized = js_sys::JSON::stringify(&value).ok()?.as_string()?;
    serde_json::from_str(&serialized).ok()
}

#[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
fn defaults_from_global() -> Option<Value> {
    None
}

fn parse_json_value(raw: String) -> Option<Value> {
    serde_json::from_str::<Value>(&raw).ok()
}

fn parse_config_source(raw: &str) -> Option<Value> {
    if let Ok(json) = serde_json::from_str::<Value>(raw) {
        if json.is_object() {
            return Some(json);
        }
    }

    if let Some(path) = treat_as_path(raw) {
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(json) = serde_json::from_str::<Value>(&contents) {
                if json.is_object() {
                    return Some(json);
                }
            }
        }
    }

    parse_key_value_config(raw)
}

#[cfg(not(target_arch = "wasm32"))]
fn treat_as_path(raw: &str) -> Option<String> {
    if raw.contains('=') {
        return None;
    }
    let trimmed = raw.trim();
    let path = Path::new(trimmed);
    if path.exists() {
        Some(trimmed.to_string())
    } else {
        None
    }
}

#[cfg(target_arch = "wasm32")]
fn treat_as_path(_raw: &str) -> Option<String> {
    None
}

fn parse_key_value_config(raw: &str) -> Option<Value> {
    let mut map = Map::new();
    for entry in raw.split(',') {
        let mut parts = entry.splitn(2, '=');
        let key = parts.next()?.trim();
        let value = parts.next()?.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

fn firebase_config_from_env() -> Option<Value> {
    if let Ok(raw) = env::var("FIREBASE_CONFIG") {
        if let Some(value) = parse_config_source(&raw) {
            return Some(value);
        }
    }

    if let Ok(raw) = env::var("FIREBASE_OPTIONS") {
        if let Some(value) = parse_config_source(&raw) {
            return Some(value);
        }
    }

    if let Ok(raw) = env::var("FIREBASE_WEBAPP_CONFIG") {
        if let Some(value) = parse_config_source(&raw) {
            return Some(value);
        }
    }

    None
}

/// Retrieves the default app configuration as a JSON map when available.
pub fn default_app_config_json() -> Option<Map<String, Value>> {
    if let Some(defaults) = firebase_defaults() {
        if let Some(config) = defaults.get("config").and_then(Value::as_object) {
            return Some(config.clone());
        }
    }

    firebase_config_from_env()?.as_object().cloned()
}

fn force_environment() -> Option<String> {
    firebase_defaults()
        .and_then(|defaults| defaults.get("forceEnvironment").cloned())
        .or_else(|| env::var("FIREBASE_ENV_FORCE").ok().map(Value::String))
        .and_then(|value| match value {
            Value::String(text) => Some(text.to_lowercase()),
            _ => None,
        })
}

/// Returns `true` if the runtime should behave as a browser environment.
pub fn is_browser() -> bool {
    if let Some(forced) = force_environment() {
        return forced == "browser";
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
    {
        use wasm_bindgen::JsCast;
        js_sys::global().dyn_into::<web_sys::Window>().is_ok()
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
    {
        false
    }
}

/// Returns `true` if the runtime appears to be a Web Worker.
pub fn is_web_worker() -> bool {
    if let Some(forced) = force_environment() {
        if forced == "browser" {
            return false;
        }
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
    {
        use wasm_bindgen::JsCast;
        js_sys::global()
            .dyn_into::<web_sys::WorkerGlobalScope>()
            .is_ok()
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_value_configs() {
        let value = parse_key_value_config("apiKey=foo,projectId=my-proj").unwrap();
        let map = value.as_object().unwrap();
        assert_eq!(map.get("apiKey").unwrap().as_str(), Some("foo"));
        assert_eq!(map.get("projectId").unwrap().as_str(), Some("my-proj"));
    }

    #[test]
    fn parse_config_source_accepts_files_and_json() {
        let json = parse_config_source("{\"apiKey\":\"foo\"}").unwrap();
        assert_eq!(json["apiKey"], "foo");

        let mut path = std::env::temp_dir();
        path.push(format!(
            "firebase_rs_sdk_test_{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, "{\"projectId\":\"demo\"}").unwrap();
        let path_str = path.to_string_lossy().to_string();
        let file_json = parse_config_source(&path_str).unwrap();
        assert_eq!(file_json["projectId"], "demo");
        let _ = fs::remove_file(path);
    }
}
