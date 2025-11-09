#![doc = include_str!("README.md")]
mod api;
mod config;
mod constants;
mod error;
mod mutation;
mod query;
mod reference;
mod transport;

pub use api::{
    connect_data_connect_emulator, execute_mutation, execute_query, get_data_connect_service,
    mutation_ref, query_ref, register_data_connect_component, subscribe, to_query_ref,
    DataConnectService,
};
pub use config::{ConnectorConfig, DataConnectOptions, TransportOptions};
pub use error::{
    internal_error, invalid_argument, operation_error, unauthorized, DataConnectError,
    DataConnectErrorCode, DataConnectOperationFailureResponse,
    DataConnectOperationFailureResponseErrorInfo, DataConnectResult,
};
pub use query::{QuerySubscriptionHandle, QuerySubscriptionHandlers};
pub use reference::{
    DataSource, MutationRef, MutationResult, QueryRef, QueryResult, SerializedQuerySnapshot,
};
pub use transport::CallerSdkType;
