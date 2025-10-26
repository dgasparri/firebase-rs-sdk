#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
use std::time::SystemTime;

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
pub fn system_time_now() -> SystemTime {
    let millis = js_sys::Date::now();
    UNIX_EPOCH + Duration::from_millis(millis as u64)
}

#[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
pub fn system_time_now() -> SystemTime {
    SystemTime::now()
}
