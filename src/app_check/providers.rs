use std::sync::Arc;

use async_trait::async_trait;

use crate::app::FirebaseApp;

use super::errors::{AppCheckError, AppCheckResult};
use super::types::{AppCheckProvider, AppCheckToken};

pub struct CustomProviderOptions {
    pub get_token: Arc<dyn Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static>,
    pub get_limited_use_token:
        Option<Arc<dyn Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static>>,
}

impl CustomProviderOptions {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static,
    {
        Self {
            get_token: Arc::new(callback),
            get_limited_use_token: None,
        }
    }

    pub fn with_limited_use<F>(mut self, callback: F) -> Self
    where
        F: Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static,
    {
        self.get_limited_use_token = Some(Arc::new(callback));
        self
    }
}

pub struct CustomProvider {
    options: CustomProviderOptions,
}

impl CustomProvider {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn() -> AppCheckResult<AppCheckToken> + Send + Sync + 'static,
    {
        Self {
            options: CustomProviderOptions::new(callback),
        }
    }

    pub fn from_options(options: CustomProviderOptions) -> Self {
        Self { options }
    }
}

#[async_trait]
impl AppCheckProvider for CustomProvider {
    async fn get_token(&self) -> AppCheckResult<AppCheckToken> {
        (self.options.get_token)()
    }

    async fn get_limited_use_token(&self) -> AppCheckResult<AppCheckToken> {
        if let Some(callback) = &self.options.get_limited_use_token {
            callback()
        } else {
            (self.options.get_token)()
        }
    }
}

pub struct ReCaptchaV3Provider {
    site_key: String,
}

impl ReCaptchaV3Provider {
    pub fn new(site_key: String) -> Self {
        Self { site_key }
    }
}

#[async_trait]
impl AppCheckProvider for ReCaptchaV3Provider {
    fn initialize(&self, _app: &FirebaseApp) {}

    async fn get_token(&self) -> AppCheckResult<AppCheckToken> {
        Err(AppCheckError::ProviderError {
            message: format!(
                "ReCAPTCHA v3 provider (site key: {}) is not implemented in the Rust port",
                self.site_key
            ),
        })
    }
}

pub struct ReCaptchaEnterpriseProvider {
    site_key: String,
}

impl ReCaptchaEnterpriseProvider {
    pub fn new(site_key: String) -> Self {
        Self { site_key }
    }
}

#[async_trait]
impl AppCheckProvider for ReCaptchaEnterpriseProvider {
    fn initialize(&self, _app: &FirebaseApp) {}

    async fn get_token(&self) -> AppCheckResult<AppCheckToken> {
        Err(AppCheckError::ProviderError {
            message: format!(
                "ReCAPTCHA Enterprise provider (site key: {}) is not implemented in the Rust port",
                self.site_key
            ),
        })
    }
}
