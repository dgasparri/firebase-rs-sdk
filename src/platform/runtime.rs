use std::fmt;
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

/// Cooperatively yields to the scheduler/event-loop in a platform-aware way.
pub async fn yield_now() {
    yield_now_impl().await;
}

/// Timeout error returned when an operation exceeds the allotted duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutError;

impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "operation timed out")
    }
}

impl std::error::Error for TimeoutError {}

/// Runs the provided future and resolves with `TimeoutError` if it does not complete
/// within the specified duration.
pub async fn with_timeout<F, T>(future: F, duration: Duration) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    if duration.is_zero() {
        return Ok(future.await);
    }

    with_timeout_impl(future, duration).await
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

#[cfg(target_arch = "wasm32")]
async fn yield_now_impl() {
    use gloo_timers::future::TimeoutFuture;
    TimeoutFuture::new(0).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn yield_now_impl() {
    tokio::task::yield_now().await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn with_timeout_impl<F, T>(future: F, duration: Duration) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    use tokio::time::timeout;

    timeout(duration, future).await.map_err(|_| TimeoutError)
}

#[cfg(target_arch = "wasm32")]
async fn with_timeout_impl<F, T>(future: F, duration: Duration) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    use futures::future::poll_fn;
    use gloo_timers::future::TimeoutFuture;
    use std::future::Future;

    let mut future = Box::pin(future);
    let timeout_ms = duration.as_millis().min(u32::MAX as u128) as u32;
    let timeout_ms = timeout_ms.max(1);
    let mut timeout_future = Box::pin(TimeoutFuture::new(timeout_ms));

    poll_fn(|cx| {
        if let std::task::Poll::Ready(result) = future.as_mut().poll(cx) {
            return std::task::Poll::Ready(Ok(result));
        }

        if let std::task::Poll::Ready(_) = timeout_future.as_mut().poll(cx) {
            return std::task::Poll::Ready(Err(TimeoutError));
        }

        std::task::Poll::Pending
    })
    .await
}
