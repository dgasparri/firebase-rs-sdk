#![doc = include_str!("README.md")]
mod api;
mod config;
mod constants;
mod error;
mod mutation;
mod query;
mod reference;
mod transport;

#[doc(inline)]
pub use api::{
    connect_data_connect_emulator, execute_mutation, execute_query, get_data_connect_service,
    mutation_ref, query_ref, register_data_connect_component, subscribe, to_query_ref,
    DataConnectQueryRuntime, DataConnectService,
};

#[doc(inline)]
pub use config::{
    parse_transport_options, ConnectorConfig, DataConnectOptions, TransportOptions,
    DEFAULT_DATA_CONNECT_HOST,
};

#[doc(inline)]
pub use constants::DATA_CONNECT_COMPONENT_NAME;

#[doc(inline)]
pub use error::{
    internal_error, invalid_argument, operation_error, unauthorized, DataConnectError,
    DataConnectErrorCode, DataConnectErrorPathSegment, DataConnectOperationFailureResponse,
    DataConnectOperationFailureResponseErrorInfo, DataConnectResult,
};

#[doc(inline)]
pub use mutation::MutationManager;

#[doc(inline)]
pub use query::{
    cache_from_serialized, QueryManager, QueryResultCallback, QuerySubscriptionHandle,
    QuerySubscriptionHandlers,
};

#[doc(inline)]
pub use reference::{
    DataSource, MutationRef, MutationResult, OpResult, OperationRef, OperationType, QueryRef,
    QueryResult, RefInfo, SerializedQuerySnapshot,
};

#[doc(inline)]
pub use transport::{
    AppCheckHeaders, CallerSdkType, DataConnectTransport, RequestTokenProvider, RestTransport,
};
