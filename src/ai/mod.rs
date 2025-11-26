#![doc = include_str!("README.md")]
mod api;
mod backend;
mod constants;
mod error;
mod helpers;
mod models;
mod public_types;
mod requests;


#[doc(inline)]
pub use api::{get_ai, get_ai_service, register_ai_component, AiService, GenerateTextRequest, GenerateTextResponse};

#[doc(inline)]
pub use backend::{Backend, BackendType, GoogleAiBackend, VertexAiBackend};

#[doc(inline)]
pub use error::{internal_error, invalid_argument, AiError, AiErrorCode, AiResult, CustomErrorData, ErrorDetails};

#[doc(inline)]
pub use models::generative_model::GenerativeModel;

#[doc(inline)]
pub use public_types::{AiOptions, AiRuntimeOptions};

#[doc(inline)]
pub use requests::{HttpMethod, PreparedRequest, RequestOptions};
