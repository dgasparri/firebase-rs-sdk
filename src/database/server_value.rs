use serde_json::Value;

/// Port of `serverTimestamp()` from
/// `packages/database/src/api/ServerValue.ts`.
pub fn server_timestamp() -> Value {
    serde_json::json!({ ".sv": "timestamp" })
}

/// Port of `increment()` from `packages/database/src/api/ServerValue.ts`.
///
/// # Arguments
/// * `delta` - Amount to atomically add to the current value.
pub fn increment(delta: f64) -> Value {
    serde_json::json!({
        ".sv": {
            "increment": delta,
        }
    })
}
