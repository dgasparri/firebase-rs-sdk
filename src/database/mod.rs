#![doc = include_str!("README.md")]
mod api;
mod backend;
mod constants;
pub mod error;
mod on_disconnect;
mod push_id;
mod query;
mod realtime;
mod server_value;

#[doc(inline)]
pub use api::{
    end_at, end_at_with_key, end_before, end_before_with_key, equal_to, equal_to_with_key, get_database,
    limit_to_first, limit_to_last, on_child_added, on_child_changed, on_child_removed, order_by_child, order_by_key,
    order_by_priority, order_by_value, push, push_with_value, query, register_database_component, run_transaction,
    set_priority, set_with_priority, start_after, start_after_with_key, start_at, start_at_with_key, ChildEvent,
    ChildEventType, DataSnapshot, Database, DatabaseQuery, DatabaseReference, ListenerRegistration, QueryConstraint,
    TransactionResult,
};

#[doc(inline)]
pub use error::DatabaseResult;

#[doc(inline)]
pub use on_disconnect::OnDisconnect;

#[doc(inline)]
pub use server_value::{increment, server_timestamp};
