mod api;
mod backend;
mod constants;
pub mod error;
mod push_id;
mod query;
mod server_value;

#[doc(inline)]
pub use api::{
    end_at, end_at_with_key, end_before, end_before_with_key, equal_to, equal_to_with_key,
    get_database, limit_to_first, limit_to_last, order_by_child, order_by_key, order_by_priority,
    order_by_value, push, push_with_value, query, register_database_component, set_priority,
    set_with_priority, start_after, start_after_with_key, start_at, start_at_with_key,
    DataSnapshot, Database, DatabaseQuery, DatabaseReference, ListenerRegistration,
    QueryConstraint,
};

#[doc(inline)]
pub use error::DatabaseResult;

#[doc(inline)]
pub use server_value::{increment, server_timestamp};
