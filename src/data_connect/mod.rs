#![doc = include_str!("README.md")]
mod api;
mod constants;
pub mod error;

pub use api::{
    get_data_connect_service, register_data_connect_component, DataConnectService, QueryRequest,
    QueryResponse,
};
