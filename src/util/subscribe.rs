use std::error::Error;
use std::sync::Arc;

pub type NextFn<T> = Arc<dyn Fn(&T) + Send + Sync + 'static>;
pub type ErrorFn = Arc<dyn Fn(&dyn Error) + Send + Sync + 'static>;
pub type CompleteFn = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Clone)]
pub struct PartialObserver<T> {
    pub next: Option<NextFn<T>>,
    pub error: Option<ErrorFn>,
    pub complete: Option<CompleteFn>,
}

impl<T> PartialObserver<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_next<F>(mut self, callback: F) -> Self
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.next = Some(Arc::new(callback));
        self
    }

    pub fn with_error<F>(mut self, callback: F) -> Self
    where
        F: Fn(&dyn Error) + Send + Sync + 'static,
    {
        self.error = Some(Arc::new(callback));
        self
    }

    pub fn with_complete<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.complete = Some(Arc::new(callback));
        self
    }
}

impl<T> Default for PartialObserver<T> {
    fn default() -> Self {
        Self {
            next: None,
            error: None,
            complete: None,
        }
    }
}

pub type Unsubscribe = Box<dyn FnOnce() + Send + 'static>;
