pub const AI_COMPONENT_NAME: &str = "ai";

/// Component/service identifier used for errors (`packages/ai/src/constants.ts`).
pub const AI_TYPE: &str = "AI";

/// Default region for Vertex AI requests.
pub const DEFAULT_LOCATION: &str = "us-central1";

/// Default domain for Firebase AI REST API requests (`packages/ai/src/constants.ts`).
pub const DEFAULT_DOMAIN: &str = "firebasevertexai.googleapis.com";

/// Default API version used by Firebase AI (`packages/ai/src/constants.ts`).
pub const DEFAULT_API_VERSION: &str = "v1beta";

/// Default fetch timeout (in milliseconds) used by the JS SDK (`packages/ai/src/constants.ts`).
pub const DEFAULT_FETCH_TIMEOUT_MS: u64 = 180_000;

/// Language tag used in the `x-goog-api-client` header.
pub const LANGUAGE_TAG: &str = "gl-rs";

/// Current crate version propagated to telemetry headers.
pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
