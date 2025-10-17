use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Number, Value};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::database::backend::{select_backend, DatabaseBackend};
use crate::database::constants::DATABASE_COMPONENT_NAME;
use crate::database::error::{internal_error, invalid_argument, DatabaseResult};
use crate::database::push_id::next_push_id;
use crate::database::query::{QueryBound, QueryIndex, QueryLimit, QueryParams};

#[derive(Clone, Debug)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

struct DatabaseInner {
    app: FirebaseApp,
    backend: Arc<dyn DatabaseBackend>,
    listeners: Mutex<HashMap<u64, Listener>>,
    next_listener_id: AtomicU64,
}

impl fmt::Debug for DatabaseInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseInner")
            .field("app", &self.app.name())
            .field("backend", &"dynamic")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct DatabaseReference {
    database: Database,
    path: Vec<String>,
}

/// Represents a composable database query, analogous to the JS `QueryImpl`
/// (`packages/database/src/api/Reference_impl.ts`).
#[derive(Clone, Debug)]
pub struct DatabaseQuery {
    reference: DatabaseReference,
    params: QueryParams,
}

/// Represents a single constraint produced by helpers such as `order_by_child()`
/// (`packages/database/src/api/Reference_impl.ts`).
#[derive(Clone, Debug)]
pub struct QueryConstraint {
    kind: QueryConstraintKind,
}

#[derive(Clone, Debug)]
enum QueryConstraintKind {
    OrderByChild(String),
    OrderByKey,
    OrderByValue,
    OrderByPriority,
    Start {
        value: Value,
        name: Option<String>,
        inclusive: bool,
    },
    End {
        value: Value,
        name: Option<String>,
        inclusive: bool,
    },
    LimitFirst(u32),
    LimitLast(u32),
    EqualTo {
        value: Value,
        name: Option<String>,
    },
}

type ListenerCallback = Arc<dyn Fn(DataSnapshot) + Send + Sync>;

#[derive(Clone)]
struct Listener {
    target: ListenerTarget,
    callback: ListenerCallback,
}

#[derive(Clone)]
enum ListenerTarget {
    Reference(Vec<String>),
    Query {
        path: Vec<String>,
        params: QueryParams,
    },
}

impl ListenerTarget {
    fn matches(&self, changed_path: &[String]) -> bool {
        match self {
            ListenerTarget::Reference(path) => paths_related(path, changed_path),
            ListenerTarget::Query { path, .. } => paths_related(path, changed_path),
        }
    }
}

/// Represents a data snapshot returned to listeners, analogous to the JS
/// `DataSnapshot` type.
#[derive(Clone, Debug)]
pub struct DataSnapshot {
    reference: DatabaseReference,
    value: Value,
}

impl DataSnapshot {
    pub fn reference(&self) -> &DatabaseReference {
        &self.reference
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn exists(&self) -> bool {
        !self.value.is_null()
    }

    pub fn key(&self) -> Option<&str> {
        self.reference.key()
    }

    pub fn into_value(self) -> Value {
        self.value
    }

    /// Returns a snapshot for the provided relative path, mirroring
    /// `DataSnapshot.child(path)` in `Reference_impl.ts`.
    pub fn child(&self, relative_path: &str) -> DatabaseResult<DataSnapshot> {
        let segments = normalize_path(relative_path)?;
        let child_reference = self.reference.child(relative_path)?;
        let value = get_value_at_path(&self.value, &segments).unwrap_or(Value::Null);
        Ok(DataSnapshot {
            reference: child_reference,
            value,
        })
    }

    /// Returns whether data exists at the provided child path, mirroring the JS
    /// `DataSnapshot.hasChild()` helper.
    pub fn has_child(&self, relative_path: &str) -> DatabaseResult<bool> {
        let segments = normalize_path(relative_path)?;
        Ok(get_value_at_path(&self.value, &segments)
            .map(|value| !value.is_null())
            .unwrap_or(false))
    }

    /// Returns whether the snapshot has any direct child properties, mirroring
    /// `DataSnapshot.hasChildren()`.
    pub fn has_children(&self) -> bool {
        match extract_data_ref(&self.value) {
            Value::Object(map) => !map.is_empty(),
            Value::Array(array) => !array.is_empty(),
            _ => false,
        }
    }

    /// Returns the number of direct child properties, mirroring the JS `size` getter.
    pub fn size(&self) -> usize {
        match extract_data_ref(&self.value) {
            Value::Object(map) => map.len(),
            Value::Array(array) => array.len(),
            _ => 0,
        }
    }

    /// Returns the JSON representation (including priority metadata) of this snapshot.
    pub fn to_json(&self) -> Value {
        self.value.clone()
    }
}

/// RAII-style listener registration; dropping the handle detaches the
/// underlying listener.
pub struct ListenerRegistration {
    database: Database,
    id: Option<u64>,
}

impl ListenerRegistration {
    fn new(database: Database, id: u64) -> Self {
        Self {
            database,
            id: Some(id),
        }
    }

    pub fn detach(mut self) {
        if let Some(id) = self.id.take() {
            self.database.remove_listener(id);
        }
    }
}

impl Drop for ListenerRegistration {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            self.database.remove_listener(id);
        }
    }
}

impl QueryConstraint {
    fn new(kind: QueryConstraintKind) -> Self {
        Self { kind }
    }

    fn apply(self, query: DatabaseQuery) -> DatabaseResult<DatabaseQuery> {
        match self.kind {
            QueryConstraintKind::OrderByChild(path) => query.order_by_child(&path),
            QueryConstraintKind::OrderByKey => query.order_by_key(),
            QueryConstraintKind::OrderByValue => query.order_by_value(),
            QueryConstraintKind::OrderByPriority => query.order_by_priority(),
            QueryConstraintKind::Start {
                value,
                name,
                inclusive,
            } => {
                if inclusive {
                    query.start_at_with_key(value, name)
                } else {
                    query.start_after_with_key(value, name)
                }
            }
            QueryConstraintKind::End {
                value,
                name,
                inclusive,
            } => {
                if inclusive {
                    query.end_at_with_key(value, name)
                } else {
                    query.end_before_with_key(value, name)
                }
            }
            QueryConstraintKind::LimitFirst(limit) => query.limit_to_first(limit),
            QueryConstraintKind::LimitLast(limit) => query.limit_to_last(limit),
            QueryConstraintKind::EqualTo { value, name } => query.equal_to_with_key(value, name),
        }
    }
}

/// Creates a derived query by applying the provided constraints, following the
/// semantics of `query()` in `packages/database/src/api/Reference_impl.ts`.
pub fn query(
    reference: DatabaseReference,
    constraints: impl IntoIterator<Item = QueryConstraint>,
) -> DatabaseResult<DatabaseQuery> {
    let mut current = reference.query();
    for constraint in constraints {
        current = constraint.apply(current)?;
    }
    Ok(current)
}

/// Produces a constraint that orders the results by the specified child path.
/// Mirrors `orderByChild()` from the JS SDK.
pub fn order_by_child(path: impl Into<String>) -> QueryConstraint {
    QueryConstraint::new(QueryConstraintKind::OrderByChild(path.into()))
}

/// Produces a constraint that orders nodes by key (`orderByKey()` in JS).
pub fn order_by_key() -> QueryConstraint {
    QueryConstraint::new(QueryConstraintKind::OrderByKey)
}

/// Produces a constraint that orders nodes by priority (`orderByPriority()` in JS).
pub fn order_by_priority() -> QueryConstraint {
    QueryConstraint::new(QueryConstraintKind::OrderByPriority)
}

/// Produces a constraint that orders nodes by value (`orderByValue()` in JS).
pub fn order_by_value() -> QueryConstraint {
    QueryConstraint::new(QueryConstraintKind::OrderByValue)
}

/// Mirrors the JS `startAt()` constraint (`Reference_impl.ts`).
pub fn start_at<V>(value: V) -> QueryConstraint
where
    V: Into<Value>,
{
    QueryConstraint::new(QueryConstraintKind::Start {
        value: value.into(),
        name: None,
        inclusive: true,
    })
}

/// Mirrors the JS `startAt(value, name)` overload (`Reference_impl.ts`).
pub fn start_at_with_key<V, S>(value: V, name: S) -> QueryConstraint
where
    V: Into<Value>,
    S: Into<String>,
{
    QueryConstraint::new(QueryConstraintKind::Start {
        value: value.into(),
        name: Some(name.into()),
        inclusive: true,
    })
}

/// Mirrors the JS `startAfter()` constraint (`Reference_impl.ts`).
pub fn start_after<V>(value: V) -> QueryConstraint
where
    V: Into<Value>,
{
    QueryConstraint::new(QueryConstraintKind::Start {
        value: value.into(),
        name: None,
        inclusive: false,
    })
}

/// Mirrors the JS `startAfter(value, name)` overload (`Reference_impl.ts`).
pub fn start_after_with_key<V, S>(value: V, name: S) -> QueryConstraint
where
    V: Into<Value>,
    S: Into<String>,
{
    QueryConstraint::new(QueryConstraintKind::Start {
        value: value.into(),
        name: Some(name.into()),
        inclusive: false,
    })
}

/// Mirrors the JS `endAt()` constraint (`Reference_impl.ts`).
pub fn end_at<V>(value: V) -> QueryConstraint
where
    V: Into<Value>,
{
    QueryConstraint::new(QueryConstraintKind::End {
        value: value.into(),
        name: None,
        inclusive: true,
    })
}

/// Mirrors the JS `endAt(value, name)` overload (`Reference_impl.ts`).
pub fn end_at_with_key<V, S>(value: V, name: S) -> QueryConstraint
where
    V: Into<Value>,
    S: Into<String>,
{
    QueryConstraint::new(QueryConstraintKind::End {
        value: value.into(),
        name: Some(name.into()),
        inclusive: true,
    })
}

/// Mirrors the JS `endBefore()` constraint (`Reference_impl.ts`).
pub fn end_before<V>(value: V) -> QueryConstraint
where
    V: Into<Value>,
{
    QueryConstraint::new(QueryConstraintKind::End {
        value: value.into(),
        name: None,
        inclusive: false,
    })
}

/// Mirrors the JS `endBefore(value, name)` overload (`Reference_impl.ts`).
pub fn end_before_with_key<V, S>(value: V, name: S) -> QueryConstraint
where
    V: Into<Value>,
    S: Into<String>,
{
    QueryConstraint::new(QueryConstraintKind::End {
        value: value.into(),
        name: Some(name.into()),
        inclusive: false,
    })
}

/// Mirrors the JS `limitToFirst()` constraint (`Reference_impl.ts`).
pub fn limit_to_first(limit: u32) -> QueryConstraint {
    QueryConstraint::new(QueryConstraintKind::LimitFirst(limit))
}

/// Mirrors the JS `limitToLast()` constraint (`Reference_impl.ts`).
pub fn limit_to_last(limit: u32) -> QueryConstraint {
    QueryConstraint::new(QueryConstraintKind::LimitLast(limit))
}

/// Mirrors the JS `equalTo()` constraint (`Reference_impl.ts`).
pub fn equal_to<V>(value: V) -> QueryConstraint
where
    V: Into<Value>,
{
    QueryConstraint::new(QueryConstraintKind::EqualTo {
        value: value.into(),
        name: None,
    })
}

/// Mirrors the JS `equalTo(value, name)` overload (`Reference_impl.ts`).
pub fn equal_to_with_key<V, S>(value: V, name: S) -> QueryConstraint
where
    V: Into<Value>,
    S: Into<String>,
{
    QueryConstraint::new(QueryConstraintKind::EqualTo {
        value: value.into(),
        name: Some(name.into()),
    })
}

/// Generates a child location with an auto-generated push ID.
///
/// Mirrors the modular `push()` helper from the JS SDK
/// (`packages/database/src/api/Reference_impl.ts`).
pub fn push(reference: &DatabaseReference) -> DatabaseResult<DatabaseReference> {
    reference.push()
}

/// Generates a child location with an auto-generated push ID and writes the provided value.
///
/// Mirrors the modular `push(ref, value)` overload from the JS SDK
/// (`packages/database/src/api/Reference_impl.ts`).
pub fn push_with_value<V>(
    reference: &DatabaseReference,
    value: V,
) -> DatabaseResult<DatabaseReference>
where
    V: Into<Value>,
{
    reference.push_with_value(value)
}

/// Writes a value together with a priority, mirroring the modular `setWithPriority()` helper
/// (`packages/database/src/api/Reference_impl.ts`).
pub fn set_with_priority<V, P>(
    reference: &DatabaseReference,
    value: V,
    priority: P,
) -> DatabaseResult<()>
where
    V: Into<Value>,
    P: Into<Value>,
{
    reference.set_with_priority(value, priority)
}

/// Updates the priority for the current location, mirroring the modular `setPriority()` helper
/// (`packages/database/src/api/Reference_impl.ts`).
pub fn set_priority<P>(reference: &DatabaseReference, priority: P) -> DatabaseResult<()>
where
    P: Into<Value>,
{
    reference.set_priority(priority)
}

impl Database {
    fn new(app: FirebaseApp) -> Self {
        Self {
            inner: Arc::new(DatabaseInner {
                backend: select_backend(&app),
                app,
                listeners: Mutex::new(HashMap::new()),
                next_listener_id: AtomicU64::new(1),
            }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn reference(&self, path: &str) -> DatabaseResult<DatabaseReference> {
        let segments = normalize_path(path)?;
        Ok(DatabaseReference {
            database: self.clone(),
            path: segments,
        })
    }

    fn reference_from_segments(&self, segments: Vec<String>) -> DatabaseReference {
        DatabaseReference {
            database: self.clone(),
            path: segments,
        }
    }

    fn register_value_listener(
        &self,
        target: ListenerTarget,
        callback: ListenerCallback,
    ) -> DatabaseResult<ListenerRegistration> {
        let id = self.inner.next_listener_id.fetch_add(1, Ordering::SeqCst);

        {
            let mut listeners = self.inner.listeners.lock().unwrap();
            listeners.insert(
                id,
                Listener {
                    target: target.clone(),
                    callback: callback.clone(),
                },
            );
        }

        // Fire an initial snapshot immediately to mirror JS semantics.
        let snapshot = self.snapshot_for_target(&target)?;
        callback(snapshot);

        Ok(ListenerRegistration::new(self.clone(), id))
    }

    fn remove_listener(&self, id: u64) {
        let mut listeners = self.inner.listeners.lock().unwrap();
        listeners.remove(&id);
    }

    fn dispatch_listeners(&self, changed_path: &[String]) -> DatabaseResult<()> {
        let targets: Vec<(ListenerTarget, ListenerCallback)> = {
            let listeners = self.inner.listeners.lock().unwrap();
            listeners
                .values()
                .filter(|listener| listener.target.matches(changed_path))
                .map(|listener| (listener.target.clone(), listener.callback.clone()))
                .collect()
        };

        for (target, callback) in targets {
            let snapshot = self.snapshot_for_target(&target)?;
            callback(snapshot);
        }
        Ok(())
    }

    fn snapshot_for_target(&self, target: &ListenerTarget) -> DatabaseResult<DataSnapshot> {
        match target {
            ListenerTarget::Reference(path) => {
                let value = self.inner.backend.get(path, &[])?;
                let reference = self.reference_from_segments(path.clone());
                Ok(DataSnapshot { reference, value })
            }
            ListenerTarget::Query { path, params } => {
                let rest_params = params.to_rest_params()?;
                let value = self.inner.backend.get(path, rest_params.as_slice())?;
                let reference = self.reference_from_segments(path.clone());
                Ok(DataSnapshot { reference, value })
            }
        }
    }
}

impl DatabaseReference {
    pub fn child(&self, relative: &str) -> DatabaseResult<DatabaseReference> {
        let mut segments = self.path.clone();
        segments.extend(normalize_path(relative)?);
        Ok(DatabaseReference {
            database: self.database.clone(),
            path: segments,
        })
    }

    /// Returns the parent of this reference, mirroring `ref.parent` in the JS SDK.
    pub fn parent(&self) -> Option<DatabaseReference> {
        if self.path.is_empty() {
            None
        } else {
            let mut parent = self.path.clone();
            parent.pop();
            Some(DatabaseReference {
                database: self.database.clone(),
                path: parent,
            })
        }
    }

    /// Returns the root of the database, mirroring `ref.root` in the JS SDK.
    pub fn root(&self) -> DatabaseReference {
        DatabaseReference {
            database: self.database.clone(),
            path: Vec::new(),
        }
    }

    pub fn set(&self, value: Value) -> DatabaseResult<()> {
        let value = self.resolve_value_for_path(&self.path, value)?;
        self.database.inner.backend.set(&self.path, value)?;
        self.database.dispatch_listeners(&self.path)?;
        Ok(())
    }

    /// Creates a query anchored at this reference, mirroring the JS `query()` helper.
    pub fn query(&self) -> DatabaseQuery {
        DatabaseQuery {
            reference: self.clone(),
            params: QueryParams::default(),
        }
    }

    /// Returns a query ordered by the provided child path, mirroring `orderByChild()`.
    pub fn order_by_child(&self, path: &str) -> DatabaseResult<DatabaseQuery> {
        self.query().order_by_child(path)
    }

    /// Returns a query ordered by key, mirroring `orderByKey()`.
    pub fn order_by_key(&self) -> DatabaseResult<DatabaseQuery> {
        self.query().order_by_key()
    }

    /// Returns a query ordered by value, mirroring `orderByValue()`.
    pub fn order_by_value(&self) -> DatabaseResult<DatabaseQuery> {
        self.query().order_by_value()
    }

    /// Returns a query ordered by priority, mirroring `orderByPriority()`.
    pub fn order_by_priority(&self) -> DatabaseResult<DatabaseQuery> {
        self.query().order_by_priority()
    }

    /// Registers a value listener for this reference, mirroring `onValue()`.
    pub fn on_value<F>(&self, callback: F) -> DatabaseResult<ListenerRegistration>
    where
        F: Fn(DataSnapshot) + Send + Sync + 'static,
    {
        let user_fn: Arc<dyn Fn(DataSnapshot) + Send + Sync> = Arc::new(callback);
        let listener_cb: ListenerCallback = Arc::new(move |snapshot: DataSnapshot| {
            user_fn(snapshot);
        });
        self.database
            .register_value_listener(ListenerTarget::Reference(self.path.clone()), listener_cb)
    }

    /// Applies the provided partial updates to the current location using a single
    /// REST `PATCH` call when available.
    ///
    /// Each key represents a relative child path (e.g. `"profile/name"`).
    /// The method rejects empty keys to mirror the JS SDK behaviour.
    pub fn update(&self, updates: serde_json::Map<String, Value>) -> DatabaseResult<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut operations = Vec::with_capacity(updates.len());
        for (key, value) in updates {
            if key.trim().is_empty() {
                return Err(invalid_argument("Database update path cannot be empty"));
            }
            let mut segments = self.path.clone();
            let relative = normalize_path(&key)?;
            if relative.is_empty() {
                return Err(invalid_argument(
                    "Database update path cannot reference the current location",
                ));
            }
            segments.extend(relative);
            let resolved = self.resolve_value_for_path(&segments, value)?;
            operations.push((segments, resolved));
        }

        self.database.inner.backend.update(&self.path, operations)?;
        self.database.dispatch_listeners(&self.path)?;
        Ok(())
    }

    pub fn get(&self) -> DatabaseResult<Value> {
        self.database.inner.backend.get(&self.path, &[])
    }

    /// Deletes the value at this location using the backend's `DELETE` support.
    pub fn remove(&self) -> DatabaseResult<()> {
        self.database.inner.backend.delete(&self.path)?;
        self.database.dispatch_listeners(&self.path)?;
        Ok(())
    }

    /// Writes the provided value together with its priority, mirroring
    /// `setWithPriority()` in `packages/database/src/api/Reference_impl.ts`.
    pub fn set_with_priority<V, P>(&self, value: V, priority: P) -> DatabaseResult<()>
    where
        V: Into<Value>,
        P: Into<Value>,
    {
        let priority = priority.into();
        validate_priority_value(&priority)?;
        if matches!(self.key(), Some(".length" | ".keys")) {
            return Err(invalid_argument(
                "set_with_priority failed: read-only child key",
            ));
        }

        let value = self.resolve_value_for_path(&self.path, value.into())?;
        let payload = pack_with_priority(value, priority);
        self.database.inner.backend.set(&self.path, payload)?;
        self.database.dispatch_listeners(&self.path)?;
        Ok(())
    }

    /// Updates the priority for this location, mirroring `setPriority()` in the JS SDK.
    pub fn set_priority<P>(&self, priority: P) -> DatabaseResult<()>
    where
        P: Into<Value>,
    {
        let priority = priority.into();
        validate_priority_value(&priority)?;

        let current = self.database.inner.backend.get(&self.path, &[])?;
        let value = extract_data_owned(&current);
        let payload = pack_with_priority(value, priority);
        self.database.inner.backend.set(&self.path, payload)?;
        self.database.dispatch_listeners(&self.path)?;
        Ok(())
    }

    /// Creates a child location with an auto-generated key, mirroring `push()` from the JS SDK.
    ///
    /// Port of `push()` in `packages/database/src/api/Reference_impl.ts`.
    ///
    /// # Examples
    /// ```
    /// # use firebase_rs_sdk_unofficial::database::{DatabaseReference, DatabaseResult};
    /// # use serde_json::json;
    /// # fn demo(messages: &DatabaseReference) -> DatabaseResult<()> {
    /// let new_message = messages.push_with_value(json!({ "text": "hi" }))?;
    /// assert!(new_message.key().is_some());
    /// # Ok(())
    /// # }
    /// ```
    pub fn push(&self) -> DatabaseResult<DatabaseReference> {
        self.push_internal(None)
    }

    /// Creates a child location with an auto-generated key and writes the provided value.
    ///
    /// Mirrors the `push(ref, value)` overload from `packages/database/src/api/Reference_impl.ts`.
    pub fn push_with_value<V>(&self, value: V) -> DatabaseResult<DatabaseReference>
    where
        V: Into<Value>,
    {
        self.push_internal(Some(value.into()))
    }

    fn resolve_value_for_path(&self, path: &[String], value: Value) -> DatabaseResult<Value> {
        if contains_server_value(&value) {
            let current = self.database.inner.backend.get(path, &[])?;
            let current_ref = extract_data_ref(&current);
            resolve_server_values(value, Some(current_ref))
        } else {
            Ok(value)
        }
    }

    fn push_internal(&self, value: Option<Value>) -> DatabaseResult<DatabaseReference> {
        let timestamp = current_time_millis()?;
        let key = next_push_id(timestamp);
        let child = self.child(&key)?;
        if let Some(value) = value {
            child.set(value)?;
        }
        Ok(child)
    }

    /// Returns the last path segment (the key) for this reference, mirroring
    /// `ref.key` in the JS SDK.
    pub fn key(&self) -> Option<&str> {
        self.path.last().map(|segment| segment.as_str())
    }

    pub fn path(&self) -> String {
        if self.path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}/", self.path.join("/"))
        }
    }
}

impl DatabaseQuery {
    /// Exposes the underlying reference backing this query.
    pub fn reference(&self) -> &DatabaseReference {
        &self.reference
    }

    /// Orders children by the given path, mirroring `orderByChild()`.
    pub fn order_by_child(mut self, path: &str) -> DatabaseResult<Self> {
        validate_order_by_child_target(path)?;
        let segments = normalize_path(path)?;
        if segments.is_empty() {
            return Err(invalid_argument("orderByChild path cannot be empty"));
        }
        let joined = segments.join("/");
        self.params.set_index(QueryIndex::Child(joined))?;
        Ok(self)
    }

    /// Orders children by key, mirroring `orderByKey()`.
    pub fn order_by_key(mut self) -> DatabaseResult<Self> {
        self.params.set_index(QueryIndex::Key)?;
        Ok(self)
    }

    /// Orders children by value, mirroring `orderByValue()`.
    pub fn order_by_value(mut self) -> DatabaseResult<Self> {
        self.params.set_index(QueryIndex::Value)?;
        Ok(self)
    }

    /// Orders children by priority, mirroring `orderByPriority()`.
    pub fn order_by_priority(mut self) -> DatabaseResult<Self> {
        self.params.set_index(QueryIndex::Priority)?;
        Ok(self)
    }

    /// Applies a `startAt()` constraint to the query.
    pub fn start_at(self, value: Value) -> DatabaseResult<Self> {
        self.start_at_with_key(value, None)
    }

    /// Applies a keyed `startAt(value, name)` constraint to the query.
    pub fn start_at_with_key(mut self, value: Value, name: Option<String>) -> DatabaseResult<Self> {
        let bound = QueryBound {
            value,
            name,
            inclusive: true,
        };
        self.params.set_start(bound)?;
        Ok(self)
    }

    /// Applies a `startAfter()` constraint to the query.
    pub fn start_after(self, value: Value) -> DatabaseResult<Self> {
        self.start_after_with_key(value, None)
    }

    /// Applies a keyed `startAfter(value, name)` constraint to the query.
    pub fn start_after_with_key(
        mut self,
        value: Value,
        name: Option<String>,
    ) -> DatabaseResult<Self> {
        let bound = QueryBound {
            value,
            name,
            inclusive: false,
        };
        self.params.set_start(bound)?;
        Ok(self)
    }

    /// Applies an `endAt()` constraint to the query.
    pub fn end_at(self, value: Value) -> DatabaseResult<Self> {
        self.end_at_with_key(value, None)
    }

    /// Applies a keyed `endAt(value, name)` constraint to the query.
    pub fn end_at_with_key(mut self, value: Value, name: Option<String>) -> DatabaseResult<Self> {
        let bound = QueryBound {
            value,
            name,
            inclusive: true,
        };
        self.params.set_end(bound)?;
        Ok(self)
    }

    /// Applies an `endBefore()` constraint to the query.
    pub fn end_before(self, value: Value) -> DatabaseResult<Self> {
        self.end_before_with_key(value, None)
    }

    /// Applies a keyed `endBefore(value, name)` constraint to the query.
    pub fn end_before_with_key(
        mut self,
        value: Value,
        name: Option<String>,
    ) -> DatabaseResult<Self> {
        let bound = QueryBound {
            value,
            name,
            inclusive: false,
        };
        self.params.set_end(bound)?;
        Ok(self)
    }

    /// Applies `limitToFirst()` to the query.
    pub fn limit_to_first(mut self, limit: u32) -> DatabaseResult<Self> {
        if limit == 0 {
            return Err(invalid_argument("limitToFirst must be greater than zero"));
        }
        self.params.set_limit(QueryLimit::First(limit))?;
        Ok(self)
    }

    /// Applies `limitToLast()` to the query.
    pub fn limit_to_last(mut self, limit: u32) -> DatabaseResult<Self> {
        if limit == 0 {
            return Err(invalid_argument("limitToLast must be greater than zero"));
        }
        self.params.set_limit(QueryLimit::Last(limit))?;
        Ok(self)
    }

    /// Applies `equalTo()` to the query.
    pub fn equal_to(self, value: Value) -> DatabaseResult<Self> {
        self.equal_to_with_key(value, None)
    }

    /// Applies a keyed `equalTo(value, name)` constraint to the query.
    pub fn equal_to_with_key(mut self, value: Value, name: Option<String>) -> DatabaseResult<Self> {
        let start_bound = QueryBound {
            value: value.clone(),
            name: name.clone(),
            inclusive: true,
        };
        let end_bound = QueryBound {
            value,
            name,
            inclusive: true,
        };
        self.params.set_start(start_bound)?;
        self.params.set_end(end_bound)?;
        Ok(self)
    }

    /// Executes the query and returns the JSON payload, mirroring JS `get()`.
    pub fn get(&self) -> DatabaseResult<Value> {
        let params = self.params.to_rest_params()?;
        self.reference
            .database
            .inner
            .backend
            .get(&self.reference.path, params.as_slice())
    }

    /// Registers a value listener for this query, mirroring `onValue(query, cb)`.
    pub fn on_value<F>(&self, callback: F) -> DatabaseResult<ListenerRegistration>
    where
        F: Fn(DataSnapshot) + Send + Sync + 'static,
    {
        let user_fn: Arc<dyn Fn(DataSnapshot) + Send + Sync> = Arc::new(callback);
        let listener_cb: ListenerCallback = Arc::new(move |snapshot: DataSnapshot| {
            user_fn(snapshot);
        });
        self.reference.database.register_value_listener(
            ListenerTarget::Query {
                path: self.reference.path.clone(),
                params: self.params.clone(),
            },
            listener_cb,
        )
    }
}

fn normalize_path(path: &str) -> DatabaseResult<Vec<String>> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut segments = Vec::new();
    for segment in trimmed.split('/') {
        if segment.is_empty() {
            return Err(invalid_argument(
                "Database path cannot contain empty segments",
            ));
        }
        segments.push(segment.to_string());
    }
    Ok(segments)
}

fn validate_order_by_child_target(path: &str) -> DatabaseResult<()> {
    match path {
        "$key" => Err(invalid_argument(
            "order_by_child(\"$key\") is invalid; call order_by_key() instead",
        )),
        "$priority" => Err(invalid_argument(
            "order_by_child(\"$priority\") is invalid; call order_by_priority() instead",
        )),
        "$value" => Err(invalid_argument(
            "order_by_child(\"$value\") is invalid; call order_by_value() instead",
        )),
        _ => Ok(()),
    }
}

fn paths_related(a: &[String], b: &[String]) -> bool {
    is_prefix(a, b) || is_prefix(b, a)
}

fn is_prefix(prefix: &[String], path: &[String]) -> bool {
    if prefix.len() > path.len() {
        return false;
    }
    prefix
        .iter()
        .zip(path.iter())
        .all(|(left, right)| left == right)
}

fn validate_priority_value(priority: &Value) -> DatabaseResult<()> {
    match priority {
        Value::Null | Value::Number(_) | Value::String(_) => Ok(()),
        _ => Err(invalid_argument(
            "Priority must be a string, number, or null",
        )),
    }
}

fn pack_with_priority(value: Value, priority: Value) -> Value {
    let mut map = Map::with_capacity(2);
    map.insert(".value".to_string(), value);
    map.insert(".priority".to_string(), priority);
    Value::Object(map)
}

fn extract_data_ref<'a>(value: &'a Value) -> &'a Value {
    value
        .as_object()
        .and_then(|obj| obj.get(".value"))
        .unwrap_or(value)
}

fn extract_data_owned(value: &Value) -> Value {
    extract_data_ref(value).clone()
}

fn contains_server_value(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            if map.contains_key(".sv") {
                return true;
            }
            map.values().any(contains_server_value)
        }
        Value::Array(items) => items.iter().any(contains_server_value),
        _ => false,
    }
}

fn resolve_server_values(value: Value, current: Option<&Value>) -> DatabaseResult<Value> {
    match value {
        Value::Object(mut map) => {
            if let Some(spec) = map.remove(".sv") {
                return resolve_server_placeholder(spec, current.map(extract_data_ref));
            }
            let mut resolved = Map::with_capacity(map.len());
            for (key, child) in map.into_iter() {
                let child_current = current
                    .and_then(|curr| match curr {
                        Value::Object(obj) => obj.get(&key),
                        Value::Array(arr) => key.parse::<usize>().ok().and_then(|idx| arr.get(idx)),
                        _ => None,
                    })
                    .map(extract_data_ref);
                let child_resolved = resolve_server_values(child, child_current)?;
                resolved.insert(key, child_resolved);
            }
            Ok(Value::Object(resolved))
        }
        Value::Array(items) => {
            let mut resolved = Vec::with_capacity(items.len());
            for (index, child) in items.into_iter().enumerate() {
                let child_current = current
                    .and_then(|curr| match curr {
                        Value::Array(arr) => arr.get(index),
                        _ => None,
                    })
                    .map(extract_data_ref);
                resolved.push(resolve_server_values(child, child_current)?);
            }
            Ok(Value::Array(resolved))
        }
        other => Ok(other),
    }
}

fn resolve_server_placeholder(spec: Value, current: Option<&Value>) -> DatabaseResult<Value> {
    match spec {
        Value::String(token) if token == "timestamp" => {
            let millis = current_time_millis()?;
            Ok(Value::Number(Number::from(millis)))
        }
        Value::Object(mut map) => {
            if let Some(delta) = map.remove("increment") {
                let delta = delta.as_f64().ok_or_else(|| {
                    invalid_argument("ServerValue.increment delta must be numeric")
                })?;
                let base = current
                    .and_then(|value| match value {
                        Value::Number(number) => number.as_f64(),
                        _ => None,
                    })
                    .unwrap_or(0.0);
                let total = base + delta;
                let number = Number::from_f64(total).ok_or_else(|| {
                    invalid_argument("ServerValue.increment produced an invalid number")
                })?;
                Ok(Value::Number(number))
            } else {
                Err(invalid_argument("Unsupported server value placeholder"))
            }
        }
        _ => Err(invalid_argument("Unsupported server value placeholder")),
    }
}

fn get_value_at_path(root: &Value, segments: &[String]) -> Option<Value> {
    fn traverse<'a>(current: &'a Value, segments: &[String]) -> Option<&'a Value> {
        if segments.is_empty() {
            return Some(current);
        }

        let data = extract_data_ref(current);
        let (first, rest) = segments.split_first().unwrap();

        match data {
            Value::Object(map) => map.get(first).and_then(|child| traverse(child, rest)),
            Value::Array(array) => {
                let index = first.parse::<usize>().ok()?;
                array.get(index).and_then(|child| traverse(child, rest))
            }
            _ => None,
        }
    }

    traverse(root, segments).map(Value::clone)
}

fn current_time_millis() -> DatabaseResult<u64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| internal_error("System time is before the Unix epoch"))?;
    let millis = duration.as_millis();
    millis
        .try_into()
        .map_err(|_| internal_error("Timestamp exceeds 64-bit range"))
}

static DATABASE_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        DATABASE_COMPONENT_NAME,
        Arc::new(database_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = app::registry::register_component(component);
});

fn database_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: DATABASE_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let database = Database::new((*app).clone());
    Ok(Arc::new(database) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&DATABASE_COMPONENT);
}

pub fn register_database_component() {
    ensure_registered();
}

pub fn get_database(app: Option<FirebaseApp>) -> DatabaseResult<Arc<Database>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, DATABASE_COMPONENT_NAME);
    if let Some(database) = provider.get_immediate::<Database>() {
        return Ok(database);
    }

    match provider.initialize::<Database>(Value::Null, None) {
        Ok(service) => Ok(service),
        Err(crate::component::types::ComponentError::InstanceUnavailable { .. }) => provider
            .get_immediate::<Database>()
            .ok_or_else(|| internal_error("Database component not available")),
        Err(err) => Err(internal_error(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use crate::database::{
        equal_to_with_key, increment, limit_to_first, limit_to_last, order_by_child, order_by_key,
        query as compose_query, server_timestamp, start_at,
    };
    use httpmock::prelude::*;
    use httpmock::Method::{DELETE, GET, PATCH, PUT};
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "database-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn set_and_get_value() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let ref_root = database.reference("/messages").unwrap();
        ref_root.set(json!({ "greeting": "hello" })).expect("set");
        let value = ref_root.get().unwrap();
        assert_eq!(value, json!({ "greeting": "hello" }));
    }

    #[test]
    fn push_generates_monotonic_keys() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let queue = database.reference("queue").unwrap();

        let keys: Vec<String> = (0..5)
            .map(|_| queue.push().unwrap().key().unwrap().to_string())
            .collect();

        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn push_with_value_persists_data() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let messages = database.reference("messages").unwrap();

        let payload = json!({ "text": "hello" });
        let child = messages
            .push_with_value(payload.clone())
            .expect("push with value");

        let stored = child.get().unwrap();
        assert_eq!(stored, payload);

        let parent = messages.get().unwrap();
        let key = child.key().unwrap();
        assert_eq!(parent.get(key), Some(&payload));
    }

    #[test]
    fn child_updates_merge() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let root = database.reference("items").unwrap();
        root.set(json!({ "a": { "count": 1 } })).unwrap();
        root.child("a/count").unwrap().set(json!(2)).unwrap();
        let value = root.get().unwrap();
        assert_eq!(value, json!({ "a": { "count": 2 } }));
    }

    #[test]
    fn set_with_priority_wraps_value() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let item = database.reference("items/main").unwrap();

        item.set_with_priority(json!({ "count": 1 }), json!(5))
            .unwrap();

        let stored = item.get().unwrap();
        assert_eq!(
            stored,
            json!({
                ".value": { "count": 1 },
                ".priority": 5
            })
        );
    }

    #[test]
    fn set_priority_updates_existing_value() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let item = database.reference("items/main").unwrap();

        item.set(json!({ "count": 4 })).unwrap();
        item.set_priority(json!(10)).unwrap();

        let stored = item.get().unwrap();
        assert_eq!(
            stored,
            json!({
                ".value": { "count": 4 },
                ".priority": 10
            })
        );
    }

    #[test]
    fn server_timestamp_is_resolved_on_set() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let created_at = database.reference("meta/created_at").unwrap();

        created_at.set(server_timestamp()).unwrap();

        let value = created_at.get().unwrap();
        let ts = value.as_u64().expect("timestamp as u64");
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(now >= ts);
        assert!(now - ts < 5_000);
    }

    #[test]
    fn server_increment_updates_value() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let counter = database.reference("counters/main").unwrap();

        counter.set(json!(1)).unwrap();
        counter.set(increment(2.0)).unwrap();

        let value = counter.get().unwrap();
        assert_eq!(value.as_f64().unwrap(), 3.0);
    }

    #[test]
    fn update_supports_server_increment() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let scores = database.reference("scores").unwrap();

        scores.set(json!({ "alice": 4 })).unwrap();
        let mut delta = serde_json::Map::new();
        delta.insert("alice".to_string(), increment(3.0));
        scores.update(delta).unwrap();

        let stored = scores.get().unwrap();
        assert_eq!(stored.get("alice").unwrap().as_f64().unwrap(), 7.0);
    }

    #[test]
    fn update_rejects_empty_key() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("items").unwrap();

        let mut updates = serde_json::Map::new();
        updates.insert("".to_string(), json!(1));

        let err = reference.update(updates).unwrap_err();
        assert_eq!(err.code_str(), "database/invalid-argument");
    }

    #[test]
    fn rest_backend_performs_http_requests() {
        let server = MockServer::start();

        let set_mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/messages.json")
                .query_param("print", "silent")
                .json_body(json!({ "greeting": "hello" }));
            then.status(200)
                .header("content-type", "application/json")
                .body("null");
        });

        let get_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/messages.json")
                .query_param("format", "export");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"greeting":"hello"}"#);
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            database_url: Some(server.url("/")),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("/messages").unwrap();

        reference
            .set(json!({ "greeting": "hello" }))
            .expect("set over REST");
        let value = reference.get().expect("get over REST");

        assert_eq!(value, json!({ "greeting": "hello" }));
        set_mock.assert();
        get_mock.assert();
    }

    #[test]
    fn reference_parent_and_root() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();

        let nested = database.reference("users/alice/profile").unwrap();
        let parent = nested.parent().expect("parent reference");
        assert_eq!(parent.path(), "/users/alice/");
        assert_eq!(parent.parent().unwrap().path(), "/users/");

        let root = nested.root();
        assert_eq!(root.path(), "/");
        assert!(root.parent().is_none());
    }

    #[test]
    fn datasnapshot_child_and_metadata_helpers() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let profiles = database.reference("profiles").unwrap();

        profiles
            .set(json!({
                "alice": { "age": 31, "city": "Rome" },
                "bob": { "age": 29 }
            }))
            .unwrap();

        let captured = Arc::new(Mutex::new(None));
        let holder = captured.clone();
        profiles
            .on_value(move |snapshot| {
                *holder.lock().unwrap() = Some(snapshot);
            })
            .unwrap();

        let snapshot = captured.lock().unwrap().clone().expect("initial snapshot");
        assert!(snapshot.exists());
        assert!(snapshot.has_children());
        assert_eq!(snapshot.size(), 2);

        let alice = snapshot.child("alice").unwrap();
        assert_eq!(alice.key(), Some("alice"));
        assert_eq!(alice.size(), 2);
        assert!(alice.has_children());
        assert_eq!(alice.child("age").unwrap().value(), &json!(31));
        assert!(snapshot.has_child("bob").unwrap());
        assert!(!snapshot.has_child("carol").unwrap());

        let json_output = snapshot.to_json();
        assert_eq!(
            json_output,
            json!({
                "alice": { "age": 31, "city": "Rome" },
                "bob": { "age": 29 }
            })
        );
    }

    #[test]
    fn rest_backend_set_with_priority_includes_metadata() {
        let server = MockServer::start();

        let put_mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/items.json")
                .query_param("print", "silent")
                .json_body(json!({
                    ".value": { "count": 1 },
                    ".priority": 3
                }));
            then.status(200)
                .header("content-type", "application/json")
                .body("null");
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            database_url: Some(server.url("/")),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("items").unwrap();

        reference
            .set_with_priority(json!({ "count": 1 }), json!(3))
            .unwrap();

        put_mock.assert();
    }

    #[test]
    fn push_with_value_rest_backend_performs_put() {
        let server = MockServer::start();

        let push_mock = server.mock(|when, then| {
            when.method(PUT)
                .path_contains("/messages/")
                .query_param("print", "silent")
                .json_body(json!({ "text": "hello" }));
            then.status(200)
                .header("content-type", "application/json")
                .body("null");
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            database_url: Some(server.url("/")),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let messages = database.reference("messages").unwrap();

        let child = messages
            .push_with_value(json!({ "text": "hello" }))
            .expect("push with value rest");

        assert_eq!(child.key().unwrap().len(), 20);
        push_mock.assert();
    }

    #[test]
    fn rest_backend_uses_patch_for_updates() {
        let server = MockServer::start();

        let patch_mock = server.mock(|when, then| {
            when.method(PATCH)
                .path("/items.json")
                .query_param("print", "silent")
                .json_body(json!({ "a/count": 2, "b": { "flag": true } }));
            then.status(200)
                .header("content-type", "application/json")
                .body("null");
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            database_url: Some(server.url("/")),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("items").unwrap();

        let mut updates = serde_json::Map::new();
        updates.insert("a/count".to_string(), json!(2));
        updates.insert("b".to_string(), json!({ "flag": true }));
        reference.update(updates).expect("patch update");

        patch_mock.assert();
    }

    #[test]
    fn rest_backend_delete_supports_remove() {
        let server = MockServer::start();

        let delete_mock = server.mock(|when, then| {
            when.method(DELETE)
                .path("/items.json")
                .query_param("print", "silent");
            then.status(200).body("null");
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            database_url: Some(server.url("/")),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("items").unwrap();

        reference.remove().expect("delete request");
        delete_mock.assert();
    }

    #[test]
    fn rest_backend_preserves_namespace_query_parameter() {
        let server = MockServer::start();

        let set_mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/messages.json")
                .query_param("ns", "demo-ns")
                .query_param("print", "silent")
                .json_body(json!({ "value": 1 }));
            then.status(200).body("null");
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            database_url: Some(format!("{}?ns=demo-ns", server.url("/"))),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("messages").unwrap();

        reference.set(json!({ "value": 1 })).unwrap();
        set_mock.assert();
    }

    #[test]
    fn rest_query_order_by_child_and_limit() {
        let server = MockServer::start();

        let get_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/items.json")
                .query_param("orderBy", "\"score\"")
                .query_param("startAt", "100")
                .query_param("limitToFirst", "5")
                .query_param("format", "export");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"a":{"score":120}}"#);
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            database_url: Some(server.url("/")),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("items").unwrap();
        let filtered = compose_query(
            reference,
            vec![order_by_child("score"), start_at(100), limit_to_first(5)],
        )
        .expect("compose query with constraints");

        let value = filtered.get().unwrap();
        assert_eq!(value, json!({ "a": { "score": 120 } }));
        get_mock.assert();
    }

    #[test]
    fn rest_query_equal_to_with_key() {
        let server = MockServer::start();

        let get_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/items.json")
                .query_param("orderBy", "\"$key\"")
                .query_param("startAt", "\"item-1\",\"item-1\"")
                .query_param("endAt", "\"item-1\",\"item-1\"")
                .query_param("format", "export");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"item-1":{"value":true}}"#);
        });

        let options = FirebaseOptions {
            project_id: Some("project".into()),
            database_url: Some(server.url("/")),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let filtered = compose_query(
            database.reference("items").unwrap(),
            vec![order_by_key(), equal_to_with_key("item-1", "item-1")],
        )
        .expect("compose equal_to query");

        let value = filtered.get().unwrap();
        assert_eq!(value, json!({ "item-1": { "value": true } }));
        get_mock.assert();
    }

    #[test]
    fn limit_to_first_rejects_zero() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();

        let err = database
            .reference("items")
            .unwrap()
            .query()
            .limit_to_first(0)
            .unwrap_err();

        assert_eq!(err.code_str(), "database/invalid-argument");
    }

    #[test]
    fn order_by_child_rejects_empty_path() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();

        let err = database
            .reference("items")
            .unwrap()
            .order_by_child("")
            .unwrap_err();

        assert_eq!(err.code_str(), "database/invalid-argument");
    }

    #[test]
    fn query_rejects_multiple_order_by_constraints() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("items").unwrap();

        let err =
            compose_query(reference, vec![order_by_key(), order_by_child("score")]).unwrap_err();

        assert_eq!(err.code_str(), "database/invalid-argument");
    }

    #[test]
    fn on_value_listener_receives_updates() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let reference = database.reference("counters/main").unwrap();

        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let captured = events.clone();

        let registration = reference
            .on_value(move |snapshot| {
                captured.lock().unwrap().push(snapshot.value().clone());
            })
            .expect("on_value registration");

        reference.set(json!(1)).unwrap();
        reference.set(json!(2)).unwrap();

        {
            let events = events.lock().unwrap();
            assert_eq!(events.len(), 3);
            assert_eq!(events[0], Value::Null);
            assert_eq!(events[1], json!(1));
            assert_eq!(events[2], json!(2));
        }

        registration.detach();
        reference.set(json!(3)).unwrap();

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn query_on_value_reacts_to_changes() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let database = get_database(Some(app)).unwrap();
        let scores = database.reference("scores").unwrap();

        scores
            .set(json!({
                "a": { "score": 10 },
                "b": { "score": 20 },
                "c": { "score": 30 }
            }))
            .unwrap();

        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let captured = events.clone();

        let _registration = compose_query(
            scores.clone(),
            vec![order_by_child("score"), limit_to_last(1)],
        )
        .unwrap()
        .on_value(move |snapshot| {
            captured.lock().unwrap().push(snapshot.value().clone());
        })
        .unwrap();

        {
            let events = events.lock().unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(
                events[0],
                json!({
                    "a": { "score": 10 },
                    "b": { "score": 20 },
                    "c": { "score": 30 }
                })
            );
        }

        scores
            .child("d")
            .unwrap()
            .set(json!({ "score": 50 }))
            .unwrap();

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[1],
            json!({
                "a": { "score": 10 },
                "b": { "score": 20 },
                "c": { "score": 30 },
                "d": { "score": 50 }
            })
        );
    }
}
