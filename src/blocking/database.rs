use super::{RT, block_on, block_on_methods};

// Sync API re-exports
pub use crate::database::error;
pub use crate::database::error::DatabaseResult;
pub use crate::database::server_value::{increment, server_timestamp};
pub use crate::database::api::{
    end_at, end_at_with_key, end_before, end_before_with_key, equal_to, equal_to_with_key,
     limit_to_first, limit_to_last,  order_by_child, order_by_key, order_by_priority, order_by_value, 
     query, register_database_component,  
     start_after, start_after_with_key, start_at, start_at_with_key, ChildEvent,
    ChildEventType, DataSnapshot, ListenerRegistration,
    QueryConstraint, TransactionResult,
};



pub fn get_database(app: Option<FirebaseApp>) -> DatabaseResult<Arc<Database>> {
    block_on(crate::database::api::get_database_async(app))
}

pub fn push(reference: &DatabaseReference) -> DatabaseResult<DatabaseReference> {
    block_on(reference.push_async())
}

pub fn push_with_value<V>(
    reference: &DatabaseReference,
    value: V,
) -> DatabaseResult<DatabaseReference>
where
    V: Into<Value>,
{
    block_on(reference.push_with_value_async(value))
}


pub fn on_child_added<F>(
    reference: &DatabaseReference,
    callback: F,
) -> DatabaseResult<ListenerRegistration>
where
    F: Fn(Result<ChildEvent, DatabaseError>) + Send + Sync + 'static,
{
    block_on(reference.on_child_added(callback))
}

pub fn on_child_changed<F>(
    reference: &DatabaseReference,
    callback: F,
) -> DatabaseResult<ListenerRegistration>
where
    F: Fn(Result<ChildEvent, DatabaseError>) + Send + Sync + 'static,
{
    block_on(reference.on_child_changed(callback))
}

pub fn on_child_removed<F>(
    reference: &DatabaseReference,
    callback: F,
) -> DatabaseResult<ListenerRegistration>
where
    F: Fn(Result<ChildEvent, DatabaseError>) + Send + Sync + 'static,
{
    block_on(reference.on_child_removed(callback))
}


pub fn run_transaction<F>(
    reference: &DatabaseReference,
    mut update: F,
) -> DatabaseResult<TransactionResult>
where
    F: FnMut(Value) -> Option<Value>,
{
    block_on(reference.run_transaction(|value| update(value)))
}

pub fn set_priority<P>(reference: &DatabaseReference, priority: P) -> DatabaseResult<()>
where
    P: Into<Value>,
{
    block_on(reference.set_priority(priority))
}


pub fn set_with_priority<V, P>(
    reference: &DatabaseReference,
    value: V,
    priority: P,
) -> DatabaseResult<()>
where
    V: Into<Value>,
    P: Into<Value>,
{
    block_on(reference.set_with_priority(value, priority))
}




struct Database {
    inner: crate::database::api::Database,
}

impl Database {
    fn new(app: FirebaseApp) -> Self {
        Self {
            inner: crate::database::api::Database::new(app),
        }
    }

    block_on_methods! {
        fn handle_realtime_action(&self, action: &str, body: &serde_json::Value) -> DatabaseResult<()>;
        fn handle_realtime_data(&self, action: &str, body: &serde_json::Value) -> DatabaseResult<()>;
        fn register_listener(&self, target: ListenerTarget, kind: ListenerKind) -> DatabaseResult<ListenerRegistration>;
        fn revoke_listener(&self, body: &serde_json::Value);
        fn dispatch_listeners(&self, changed_path: &[String], old_root: &Value, new_root: &Value) -> DatabaseResult<()>;
        fn root_snapshot(&self) -> DatabaseResult<Value> ;
        fn snapshot_from_root(&self,target: &ListenerTarget,root: &Value) -> DatabaseResult<DataSnapshot>;
        fn snapshot_for_target(&self, target: &ListenerTarget) -> DatabaseResult<DataSnapshot>;
        fn go_online() -> DatabaseResult<()>;
        fn go_offline() -> DatabaseResult<()>;
    }
}

impl Deref for Database {
    type Target = crate::database::api::Database;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}


struct DatabaseQuery {
    inner: crate::database::api::DatabaseQuery,
}

impl DatabaseQuery {
    fn new(inner: crate::database::api::DatabaseQuery) -> Self {
        Self { inner }
    }

    block_on_methods! {
        fn get(&self) -> DatabaseResult<Value> ;
        fn on_value<F>(&self, callback: F) -> DatabaseResult<ListenerRegistration> where F: Fn(Result<DataSnapshot, DatabaseError>) + Send + Sync + 'static;
    }
}

impl Deref for DatabaseQuery {
    type Target = crate::database::api::DatabaseQuery;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}


struct DatabaseReference {
    inner: crate::database::api::DatabaseReference,
}

impl DatabaseReference {
    // TODO: ma ha constructor???
    fn new(inner: crate::database::api::DatabaseReference) -> Self {
        Self { inner }
    }

    //pub(crate) async fn resolve_for_current_path(&self, value: Value) -> DatabaseResult<Value>
    //pub(crate) async fn resolve_for_absolute_path(&self,path: &[String],value: Value,) -> DatabaseResult<Value>;

    block_on_methods! {
        fn set(&self, value: Value) -> DatabaseResult<()>;
        fn on_value<F>(&self, callback: F) -> DatabaseResult<ListenerRegistration> where F: Fn(Result<DataSnapshot, DatabaseError>) + Send + Sync + 'static;
        fn on_child_added<F>(&self, callback: F) -> DatabaseResult<ListenerRegistration> where F: Fn(Result<ChildEvent, DatabaseError>) + Send + Sync + 'static;
        fn on_child_changed<F>(&self, callback: F) -> DatabaseResult<ListenerRegistration> where F: Fn(Result<ChildEvent, DatabaseError>) + Send + Sync + 'static;
        fn on_child_removed<F>(&self, callback: F) -> DatabaseResult<ListenerRegistration> where F: Fn(Result<ChildEvent, DatabaseError>) + Send + Sync + 'static;
        fn run_transaction<F>(&self, mut update: F) -> DatabaseResult<TransactionResult> where F: FnMut(Value) -> Option<Value>;
        fn update(&self, updates: serde_json::Map<String, Value>) -> DatabaseResult<()>;
        fn get(&self) -> DatabaseResult<Value>;
        fn remove(&self) -> DatabaseResult<()>;
        fn set_with_priority<V, P>(&self, value: V, priority: P) -> DatabaseResult<()> where V: Into<Value>, P: Into<Value>;
        fn push(&self) -> DatabaseResult<DatabaseReference> ;
        fn set_priority<P>(&self, priority: P) -> DatabaseResult<()> where P: Into<Value>;
        fn push_with_value<V>(&self, value: V) -> DatabaseResult<DatabaseReference> where V: Into<Value>;
    }
}

impl Deref for DatabaseReference {
    type Target = crate::database::api::DatabaseReference;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}


struct OnDisconnect {
    inner: crate::database::on_disconnect::OnDisconnect,
}

impl OnDisconnect {
    fn new(inner: crate::database::on_disconnect::OnDisconnect) -> Self {
        Self { inner }
    }

    block_on_methods! {
        fn set<V>(&self, value: V) -> DatabaseResult<()> where V: Into<Value>;
        fn set_with_priority<V, P>(&self, value: V, priority: P) -> DatabaseResult<()> where V: Into<Value>, P: Into<Value>;
        fn update(&self, updates: serde_json::Map<String, Value>) -> DatabaseResult<()>;
        fn remove(&self) -> DatabaseResult<()>;
        fn cancel(&self) -> DatabaseResult<()>;
    }
}

impl Deref for OnDisconnect {
    type Target = crate::database::on_disconnect::OnDisconnect;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}






