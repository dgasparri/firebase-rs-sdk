use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app_check::errors::AppCheckError;
use crate::platform::runtime::{sleep, spawn_detached};

#[cfg(not(target_arch = "wasm32"))]
type OperationFuture = Pin<Box<dyn Future<Output = Result<(), AppCheckError>> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
type OperationFuture = Pin<Box<dyn Future<Output = Result<(), AppCheckError>> + 'static>>;

#[cfg(not(target_arch = "wasm32"))]
type OperationFn = Arc<dyn Fn() -> OperationFuture + Send + Sync>;
#[cfg(target_arch = "wasm32")]
type OperationFn = Arc<dyn Fn() -> OperationFuture + Send + Sync>;

type RetryPolicyFn = Arc<dyn Fn(&AppCheckError) -> bool + Send + Sync>;
type WaitDurationFn = Arc<dyn Fn() -> Duration + Send + Sync>;

struct RefresherInner {
    operation: OperationFn,
    retry_policy: RetryPolicyFn,
    wait_duration: WaitDurationFn,
    lower_bound: Duration,
    upper_bound: Duration,
    next_error_wait: Mutex<Duration>,
    running: AtomicBool,
    cancel_requested: AtomicBool,
}

impl RefresherInner {
    fn next_wait(&self, succeeded: bool) -> Duration {
        if succeeded {
            *self.next_error_wait.lock().unwrap() = self.lower_bound;
            (self.wait_duration)()
        } else {
            let mut guard = self.next_error_wait.lock().unwrap();
            let current = *guard;
            let mut next_millis = current.as_millis().saturating_mul(2);
            let upper_millis = self.upper_bound.as_millis();
            if next_millis > upper_millis {
                next_millis = upper_millis;
            }
            *guard = Duration::from_millis(next_millis as u64);
            current
        }
    }
}

#[derive(Clone)]
pub struct Refresher {
    inner: Arc<RefresherInner>,
}

impl Refresher {
    pub fn new(
        operation: OperationFn,
        retry_policy: RetryPolicyFn,
        wait_duration: WaitDurationFn,
        lower_bound: Duration,
        upper_bound: Duration,
    ) -> Self {
        assert!(lower_bound <= upper_bound);
        Self {
            inner: Arc::new(RefresherInner {
                operation,
                retry_policy,
                wait_duration,
                lower_bound,
                upper_bound,
                next_error_wait: Mutex::new(lower_bound),
                running: AtomicBool::new(false),
                cancel_requested: AtomicBool::new(false),
            }),
        }
    }

    pub fn start(&self) {
        if self
            .inner
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        *self.inner.next_error_wait.lock().unwrap() = self.inner.lower_bound;
        self.inner.cancel_requested.store(false, Ordering::SeqCst);

        let inner = Arc::clone(&self.inner);
        spawn_detached(async move {
            run_loop(inner).await;
        });
    }

    pub fn stop(&self) {
        self.inner.cancel_requested.store(true, Ordering::SeqCst);
        self.inner.running.store(false, Ordering::SeqCst);
    }

    // Used in state.rs tests.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::SeqCst)
    }
}

async fn run_loop(inner: Arc<RefresherInner>) {
    let mut succeeded = true;

    loop {
        if inner.cancel_requested.load(Ordering::SeqCst) {
            break;
        }

        let wait = inner.next_wait(succeeded);
        if !wait.is_zero() {
            sleep(wait).await;
        }

        if inner.cancel_requested.load(Ordering::SeqCst) {
            break;
        }

        match (inner.operation)().await {
            Ok(()) => {
                succeeded = true;
            }
            Err(error) => {
                if !(inner.retry_policy)(&error) {
                    break;
                }
                succeeded = false;
                continue;
            }
        }
    }

    inner.running.store(false, Ordering::SeqCst);
    inner.cancel_requested.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test(flavor = "current_thread")]
    async fn retries_with_backoff() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();
        let operation: OperationFn = Arc::new(move || {
            let attempts = attempts_clone.clone();
            Box::pin(async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(AppCheckError::TokenFetchFailed { message: "fail".into() })
                } else {
                    Ok(())
                }
            })
        });

        let refresher = Refresher::new(
            operation,
            Arc::new(|_| true),
            Arc::new(|| Duration::from_millis(1)),
            Duration::from_millis(1),
            Duration::from_millis(4),
        );

        refresher.start();

        for _ in 0..20 {
            if attempts.load(Ordering::SeqCst) >= 3 {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }

        refresher.stop();
        assert!(attempts.load(Ordering::SeqCst) >= 3);
    }
}
