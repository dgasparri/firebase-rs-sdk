use super::{RT, block_on, block_on_methods};

//TODO
pub use crate::database::error;



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



// Database,
// DatabaseQuery,
// DatabaseReference,
// Sync API re-exports
pub use crate::database::api::{
    end_at, end_at_with_key, end_before, end_before_with_key, equal_to, equal_to_with_key,
     limit_to_first, limit_to_last,  order_by_child, order_by_key, order_by_priority, order_by_value, 
     query, register_database_component,  
     start_after, start_after_with_key, start_at, start_at_with_key, ChildEvent,
    ChildEventType, DataSnapshot,    ListenerRegistration,
    QueryConstraint, TransactionResult,
};


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









// TODO
pub use crate::database::error::DatabaseResult;
// TODO
pub use crate::database::on_disconnect::OnDisconnect;
// TODO
pub use crate::database::server_value::{increment, server_timestamp};




impl Database {

    pub fn go_online(&self) -> DatabaseResult<()> {
        block_on(self.go_online_async())
    }

    pub fn go_offline(&self) -> DatabaseResult<()> {
        block_on(self.go_offline_async())
    }


}



impl DatabaseReference {

    pub fn get(&self) -> DatabaseResult<Value> {
        block_on(self.get_async())
    }

    pub fn push(&self) -> DatabaseResult<DatabaseReference> {
        block_on(self.push_async())
    }

    pub fn push_with_value<V>(&self, value: V) -> DatabaseResult<DatabaseReference>
    where
        V: Into<Value>,
    {
        block_on(self.push_with_value_async(value))
    }

    pub fn remove(&self) -> DatabaseResult<()> {
        block_on(self.remove_async())
    }


    pub fn set(&self, value: Value) -> DatabaseResult<()> {
        block_on(self.set_async(value))
    }

    pub fn set_priority<P>(&self, priority: P) -> DatabaseResult<()>
    where
        P: Into<Value>,
    {
        block_on(self.set_priority_async(priority))
    }


    pub fn set_with_priority<V, P>(&self, value: V, priority: P) -> DatabaseResult<()>
    where
        V: Into<Value>,
        P: Into<Value>,
    {
        block_on(self.set_with_priority_async(value, priority))
    }


    pub fn update(&self, updates: serde_json::Map<String, Value>) -> DatabaseResult<()> {
        block_on(self.update_async(updates))
    }


}

impl DatabaseQuery {

    pub fn get(&self) -> DatabaseResult<Value> {
        block_on(self.get_async())
    }

}