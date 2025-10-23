//! Shared runtime helpers for bridging async code in synchronous contexts.

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::future::Future;
    use std::sync::LazyLock;

    use tokio::runtime::{Builder, Runtime};

    static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
        Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime")
    });

    /// Blocks the current thread on the provided future using a shared Tokio runtime.
    pub fn block_on<F, T>(future: F) -> T
    where
        F: Future<Output = T> + 'static,
        T: 'static,
    {
        RUNTIME.block_on(future)
    }
}

#[cfg(target_arch = "wasm32")]
pub mod native {
    use std::future::Future;

    /// Blocking on futures is not supported in wasm builds; callers should rely on async APIs.
    pub fn block_on<F, T>(_future: F) -> T
    where
        F: Future<Output = T>,
    {
        panic!("blocking on futures is unsupported in wasm builds")
    }
}

pub use native::block_on;
