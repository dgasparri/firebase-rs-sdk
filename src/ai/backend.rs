use std::fmt;

use crate::ai::constants::DEFAULT_LOCATION;

/// Supported backend types for the Firebase AI SDK.
///
/// Ported from `packages/ai/src/public-types.ts` (`BackendType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendType {
    VertexAi,
    GoogleAi,
}

impl fmt::Display for BackendType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendType::VertexAi => write!(f, "VERTEX_AI"),
            BackendType::GoogleAi => write!(f, "GOOGLE_AI"),
        }
    }
}

/// Configuration for the Gemini Developer API backend.
///
/// Mirrors `GoogleAIBackend` in `packages/ai/src/backend.ts`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GoogleAiBackend;

impl GoogleAiBackend {
    /// Creates a configuration for the Gemini Developer API backend.
    pub fn new() -> Self {
        Self
    }

    /// Returns the backend type tag.
    pub fn backend_type(&self) -> BackendType {
        BackendType::GoogleAi
    }
}

/// Configuration for the Vertex AI Gemini backend.
///
/// Mirrors `VertexAIBackend` in `packages/ai/src/backend.ts`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexAiBackend {
    location: String,
}

impl VertexAiBackend {
    /// Creates a configuration for the Vertex AI backend.
    ///
    /// When `location` is empty the default `us-central1` region is used, matching
    /// the TypeScript implementation.
    pub fn new<S: Into<String>>(location: S) -> Self {
        let location = location.into();
        let location = if location.trim().is_empty() {
            DEFAULT_LOCATION.to_string()
        } else {
            location
        };
        Self { location }
    }

    /// Returns the configured region (e.g. `us-central1`).
    pub fn location(&self) -> &str {
        &self.location
    }

    /// Returns the backend type tag.
    pub fn backend_type(&self) -> BackendType {
        BackendType::VertexAi
    }
}

impl Default for VertexAiBackend {
    fn default() -> Self {
        Self::new(DEFAULT_LOCATION)
    }
}

/// High-level backend configuration enum used by the Rust port.
///
/// This combines the behaviour of the TypeScript `Backend` base class and its concrete
/// subclasses so we can work with a single, cloneable configuration value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Backend {
    GoogleAi(GoogleAiBackend),
    VertexAi(VertexAiBackend),
}

impl Backend {
    /// Creates a Google AI backend configuration.
    pub fn google_ai() -> Self {
        Self::GoogleAi(GoogleAiBackend::new())
    }

    /// Creates a Vertex AI backend configuration with the provided region.
    pub fn vertex_ai<S: Into<String>>(location: S) -> Self {
        Self::VertexAi(VertexAiBackend::new(location))
    }

    /// Returns the backend type tag.
    pub fn backend_type(&self) -> BackendType {
        match self {
            Backend::GoogleAi(inner) => inner.backend_type(),
            Backend::VertexAi(inner) => inner.backend_type(),
        }
    }

    /// Returns the backend as a Vertex AI configuration if applicable.
    pub fn as_vertex_ai(&self) -> Option<&VertexAiBackend> {
        match self {
            Backend::VertexAi(inner) => Some(inner),
            _ => None,
        }
    }
}

impl Default for Backend {
    fn default() -> Self {
        Backend::google_ai()
    }
}
