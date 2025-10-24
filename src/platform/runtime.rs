use std::future::Future;
use std::time::Duration;

/// Platform-independent helper to spawn an async task that runs in the background.
#[cfg(target_arch = "wasm32")]
pub fn spawn_detached<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

/// Platform-independent helper to spawn an async task that runs in the background.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_detached<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    use std::sync::LazyLock;
    use tokio::runtime::{Builder, Handle, Runtime};

    static BACKGROUND_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
        Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build background tokio runtime")
    });

    if let Ok(handle) = Handle::try_current() {
        handle.spawn(future);
    } else {
        let _ = BACKGROUND_RUNTIME.spawn(future);
    }
}

/// Asynchronously waits for the provided duration in a platform-compatible way.
pub async fn sleep(duration: Duration) {
    if duration.is_zero() {
        return;
    }

    sleep_impl(duration).await;
}

#[cfg(target_arch = "wasm32")]
async fn sleep_impl(duration: Duration) {
    use gloo_timers::future::sleep;
    sleep(duration).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep_impl(duration: Duration) {
    use tokio::time::sleep;
    sleep(duration).await;
}
