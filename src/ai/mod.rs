#![doc = include_str!("README.md")]
mod api;
pub mod backend;
mod constants;
pub mod error;
mod helpers;
mod models;
pub mod public_types;
mod requests;

pub use api::{
    get_ai, get_ai_service, register_ai_component, AiService, GenerateTextRequest,
    GenerateTextResponse,
};
pub use models::generative_model::GenerativeModel;
pub use requests::{HttpMethod, PreparedRequest, RequestOptions};
