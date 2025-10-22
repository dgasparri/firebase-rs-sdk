use crate::ai::backend::Backend;

/// Options for configuring the AI service at initialization time.
///
/// Mirrors `AIOptions` from `packages/ai/src/public-types.ts`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiOptions {
    /// Backend to use for inference. Defaults to `GoogleAIBackend` when omitted.
    pub backend: Option<Backend>,
    /// Whether to use limited-use App Check tokens for authenticated requests.
    pub use_limited_use_app_check_tokens: Option<bool>,
}

impl AiOptions {
    /// Returns the configured backend or the default Google AI backend when unset.
    pub fn backend_or_default(&self) -> Backend {
        self.backend.clone().unwrap_or_default()
    }

    /// Returns the `use_limited_use_app_check_tokens` flag, defaulting to `false`.
    pub fn limited_use_app_check(&self) -> bool {
        self.use_limited_use_app_check_tokens.unwrap_or(false)
    }
}

/// Runtime options stored on the {@link AiService} once the backend has been resolved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiRuntimeOptions {
    pub use_limited_use_app_check_tokens: bool,
}
