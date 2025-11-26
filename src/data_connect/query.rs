use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde_json::Value;

use crate::data_connect::error::{DataConnectError, DataConnectResult};
use crate::data_connect::reference::{
    encode_query_key, string_to_system_time, DataSource, OpResult, QueryRef, QueryResult, SerializedQuerySnapshot,
};
use crate::data_connect::transport::DataConnectTransport;
use crate::platform::runtime;

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
type ValueCallback = dyn Fn() + 'static;
#[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
type ValueCallback = dyn Fn() + Send + Sync + 'static;

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
type DataCallback<T> = dyn Fn(&T) + 'static;
#[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
type DataCallback<T> = dyn Fn(&T) + Send + Sync + 'static;

pub type QueryResultCallback = Arc<DataCallback<QueryResult>>;
pub type QueryErrorCallback = Arc<DataCallback<DataConnectError>>;
pub type QueryCompleteCallback = Arc<ValueCallback>;

/// Observer-style subscription handlers.
#[derive(Clone)]
pub struct QuerySubscriptionHandlers {
    pub on_next: QueryResultCallback,
    pub on_error: Option<QueryErrorCallback>,
    pub on_complete: Option<QueryCompleteCallback>,
}

impl QuerySubscriptionHandlers {
    pub fn new(on_next: QueryResultCallback) -> Self {
        Self {
            on_next,
            on_error: None,
            on_complete: None,
        }
    }

    pub fn with_error(mut self, callback: QueryErrorCallback) -> Self {
        self.on_error = Some(callback);
        self
    }

    pub fn with_complete(mut self, callback: QueryCompleteCallback) -> Self {
        self.on_complete = Some(callback);
        self
    }
}

/// Guard returned when subscribing to a query.
pub struct QuerySubscriptionHandle {
    tracked: Arc<TrackedQuery>,
    subscriber_id: u64,
    closed: AtomicBool,
}

impl QuerySubscriptionHandle {
    fn new(tracked: Arc<TrackedQuery>, subscriber_id: u64) -> Self {
        Self {
            tracked,
            subscriber_id,
            closed: AtomicBool::new(false),
        }
    }

    pub fn unsubscribe(mut self) {
        self.close();
    }

    fn close(&mut self) {
        if !self.closed.swap(true, Ordering::SeqCst) {
            self.tracked.remove_subscriber(self.subscriber_id);
        }
    }
}

impl Drop for QuerySubscriptionHandle {
    fn drop(&mut self) {
        self.close();
    }
}

/// Tracks outstanding queries, cached payloads, and subscribers.
#[derive(Clone)]
pub struct QueryManager {
    inner: Arc<QueryManagerInner>,
}

impl QueryManager {
    pub fn new(transport: Arc<dyn DataConnectTransport>) -> Self {
        Self {
            inner: Arc::new(QueryManagerInner::new(transport)),
        }
    }

    pub async fn execute_query(&self, query_ref: QueryRef) -> DataConnectResult<QueryResult> {
        self.inner.execute_query(query_ref).await
    }

    pub fn subscribe(
        &self,
        query_ref: QueryRef,
        handlers: QuerySubscriptionHandlers,
        initial_cache: Option<OpResult>,
    ) -> DataConnectResult<QuerySubscriptionHandle> {
        self.inner.subscribe(self.clone(), query_ref, handlers, initial_cache)
    }
}

struct QueryManagerInner {
    transport: Arc<dyn DataConnectTransport>,
    queries: Mutex<HashMap<String, Arc<TrackedQuery>>>,
    next_id: AtomicU64,
}

impl QueryManagerInner {
    fn new(transport: Arc<dyn DataConnectTransport>) -> Self {
        Self {
            transport,
            queries: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    fn key_for(query_ref: &QueryRef) -> String {
        encode_query_key(query_ref.operation_name(), query_ref.variables())
    }

    fn track(&self, query_ref: &QueryRef, initial_cache: Option<OpResult>) {
        let tracked = self.tracked_entry(query_ref);
        if let Some(cache) = initial_cache {
            tracked.maybe_update_cache(cache);
        }
    }

    fn tracked_entry(&self, query_ref: &QueryRef) -> Arc<TrackedQuery> {
        let key = Self::key_for(query_ref);
        let mut queries = self.queries.lock().unwrap();
        queries
            .entry(key.clone())
            .or_insert_with(|| {
                Arc::new(TrackedQuery::new(
                    key,
                    query_ref.operation_name().into(),
                    query_ref.variables().clone(),
                ))
            })
            .clone()
    }

    async fn execute_query(&self, query_ref: QueryRef) -> DataConnectResult<QueryResult> {
        let tracked = self.tracked_entry(&query_ref);
        match self
            .transport
            .invoke_query(query_ref.operation_name(), query_ref.variables())
            .await
        {
            Ok(data) => {
                let fetch_time = SystemTime::now();
                let result = QueryResult {
                    data: data.clone(),
                    source: DataSource::Server,
                    fetch_time,
                    query_ref: query_ref.clone(),
                };
                tracked.set_cache(OpResult {
                    data,
                    source: DataSource::Cache,
                    fetch_time,
                });
                tracked.clear_error();
                tracked.notify_success(&result);
                Ok(result)
            }
            Err(err) => {
                tracked.record_error(err.clone());
                Err(err)
            }
        }
    }

    fn subscribe(
        &self,
        manager: QueryManager,
        query_ref: QueryRef,
        handlers: QuerySubscriptionHandlers,
        initial_cache: Option<OpResult>,
    ) -> DataConnectResult<QuerySubscriptionHandle> {
        self.track(&query_ref, initial_cache);
        let tracked = self.tracked_entry(&query_ref);
        let subscriber_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        tracked.add_subscriber(subscriber_id, handlers.clone());

        if let Some(cache) = tracked.cache_snapshot() {
            let snapshot = QueryResult {
                data: cache.data,
                source: cache.source,
                fetch_time: cache.fetch_time,
                query_ref: query_ref.clone(),
            };
            (handlers.on_next)(&snapshot);
        } else {
            let manager_clone = manager.clone();
            let query_clone = query_ref.clone();
            runtime::spawn_detached(async move {
                let _ = manager_clone.execute_query(query_clone).await;
            });
        }

        if let Some(last_error) = tracked.last_error() {
            if let Some(on_error) = handlers.on_error {
                on_error(&last_error);
            }
        }

        Ok(QuerySubscriptionHandle::new(tracked, subscriber_id))
    }
}

struct SubscriberEntry {
    id: u64,
    handlers: QuerySubscriptionHandlers,
}

struct TrackedState {
    subscribers: Vec<SubscriberEntry>,
    current_cache: Option<OpResult>,
    last_error: Option<DataConnectError>,
}

struct TrackedQuery {
    #[allow(unused)]
    key: String,
    #[allow(unused)]
    name: Arc<str>,
    #[allow(unused)]
    variables: Value,
    state: Mutex<TrackedState>,
}

impl TrackedQuery {
    fn new(key: String, name: Arc<str>, variables: Value) -> Self {
        Self {
            key,
            name,
            variables,
            state: Mutex::new(TrackedState {
                subscribers: Vec::new(),
                current_cache: None,
                last_error: None,
            }),
        }
    }

    fn add_subscriber(&self, id: u64, handlers: QuerySubscriptionHandlers) {
        self.state
            .lock()
            .unwrap()
            .subscribers
            .push(SubscriberEntry { id, handlers });
    }

    fn remove_subscriber(&self, id: u64) {
        let mut state = self.state.lock().unwrap();
        if let Some(pos) = state.subscribers.iter().position(|entry| entry.id == id) {
            if let Some(callback) = state.subscribers[pos].handlers.on_complete.clone() {
                callback();
            }
            state.subscribers.remove(pos);
        }
    }

    fn maybe_update_cache(&self, cache: OpResult) {
        let mut state = self.state.lock().unwrap();
        match &state.current_cache {
            Some(existing) if existing.fetch_time >= cache.fetch_time => {}
            _ => state.current_cache = Some(cache),
        }
    }

    fn set_cache(&self, cache: OpResult) {
        self.state.lock().unwrap().current_cache = Some(cache);
    }

    fn cache_snapshot(&self) -> Option<OpResult> {
        self.state.lock().unwrap().current_cache.clone()
    }

    fn record_error(&self, error: DataConnectError) {
        let mut state = self.state.lock().unwrap();
        state.last_error = Some(error.clone());
        let subscribers = state
            .subscribers
            .iter()
            .map(|entry| entry.handlers.on_error.clone())
            .collect::<Vec<_>>();
        drop(state);
        for maybe_handler in subscribers {
            if let Some(handler) = maybe_handler {
                handler(&error);
            }
        }
    }

    fn last_error(&self) -> Option<DataConnectError> {
        self.state.lock().unwrap().last_error.clone()
    }

    fn clear_error(&self) {
        self.state.lock().unwrap().last_error = None;
    }

    fn notify_success(&self, result: &QueryResult) {
        let handlers = self
            .state
            .lock()
            .unwrap()
            .subscribers
            .iter()
            .map(|entry| entry.handlers.on_next.clone())
            .collect::<Vec<_>>();
        for callback in handlers {
            callback(result);
        }
    }
}

/// Converts a serialized query snapshot (e.g. produced on the server) into an initial cache entry.
pub fn cache_from_serialized(snapshot: &SerializedQuerySnapshot) -> Option<OpResult> {
    let fetch_time = string_to_system_time(&snapshot.fetch_time)?;
    Some(OpResult {
        data: snapshot.data.clone(),
        source: snapshot.source,
        fetch_time,
    })
}
