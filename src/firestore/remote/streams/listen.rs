use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_lock::Mutex;
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde_json::{json, Value as JsonValue};

use crate::firestore::api::query::QueryDefinition;
use crate::firestore::error::{internal_error, FirestoreError, FirestoreResult};
use crate::firestore::remote::datastore::StreamHandle;
use crate::firestore::remote::network::{NetworkLayer, NetworkStreamHandler, StreamCredentials};
use crate::firestore::remote::serializer::JsonProtoSerializer;
use crate::firestore::remote::stream::persistent::PersistentStreamHandle;
use crate::firestore::remote::structured_query::encode_structured_query;
use crate::firestore::remote::watch_change::{decode_watch_change, WatchChange};

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ListenStreamDelegate: Send + Sync + 'static {
    async fn on_watch_change(&self, change: WatchChange) -> FirestoreResult<()>;
    async fn on_stream_error(&self, error: FirestoreError);
}

pub struct ListenStream<D>
where
    D: ListenStreamDelegate,
{
    handler: Arc<ListenStreamHandler<D>>,
    handle: PersistentStreamHandle,
}

impl<D> ListenStream<D>
where
    D: ListenStreamDelegate,
{
    pub fn new(layer: NetworkLayer, serializer: JsonProtoSerializer, delegate: Arc<D>) -> Self {
        let handler = Arc::new(ListenStreamHandler::new(serializer, delegate));
        let handle = layer.listen(Arc::clone(&handler));
        Self { handler, handle }
    }

    pub async fn watch(&self, target: ListenTarget) -> FirestoreResult<()> {
        self.handler.watch(target).await
    }

    pub async fn unwatch(&self, target_id: i32) -> FirestoreResult<()> {
        self.handler.unwatch(target_id).await
    }

    pub fn stop(&self) {
        self.handler.stop();
        self.handle.stop();
    }
}

#[derive(Clone, Debug)]
pub struct ListenTarget {
    target_id: i32,
    payload: TargetPayload,
    resume_token: Option<Vec<u8>>,
    labels: Option<BTreeMap<String, String>>,
    once: bool,
}

impl ListenTarget {
    pub fn for_query(
        serializer: &JsonProtoSerializer,
        target_id: i32,
        definition: &QueryDefinition,
    ) -> FirestoreResult<Self> {
        let parent_path = definition.parent_path();
        let parent = if parent_path.is_empty() {
            format!("{}/documents", serializer.database_name())
        } else {
            format!(
                "{}/documents/{}",
                serializer.database_name(),
                parent_path.canonical_string()
            )
        };

        let structured_query = encode_structured_query(serializer, definition)?;
        Ok(Self {
            target_id,
            payload: TargetPayload::Query {
                parent,
                structured_query,
            },
            resume_token: None,
            labels: None,
            once: false,
        })
    }

    pub fn target_id(&self) -> i32 {
        self.target_id
    }

    pub fn payload(&self) -> &TargetPayload {
        &self.payload
    }

    pub fn resume_token(&self) -> Option<&[u8]> {
        self.resume_token.as_deref()
    }

    pub fn set_resume_token(mut self, token: Vec<u8>) -> Self {
        self.resume_token = Some(token);
        self
    }

    pub fn set_labels(mut self, labels: BTreeMap<String, String>) -> Self {
        self.labels = Some(labels);
        self
    }

    pub fn set_once(mut self, once: bool) -> Self {
        self.once = once;
        self
    }
}

#[derive(Clone, Debug)]
pub enum TargetPayload {
    Query {
        parent: String,
        structured_query: JsonValue,
    },
    Documents {
        documents: Vec<String>,
    },
}

struct ListenStreamHandler<D>
where
    D: ListenStreamDelegate,
{
    serializer: Arc<JsonProtoSerializer>,
    delegate: Arc<D>,
    state: Mutex<ListenStreamState>,
    running: AtomicBool,
}

struct ListenStreamState {
    stream: Option<Arc<dyn StreamHandle>>,
    targets: BTreeMap<i32, ListenTarget>,
}

impl<D> ListenStreamHandler<D>
where
    D: ListenStreamDelegate,
{
    fn new(serializer: JsonProtoSerializer, delegate: Arc<D>) -> Self {
        Self {
            serializer: Arc::new(serializer),
            delegate,
            state: Mutex::new(ListenStreamState {
                stream: None,
                targets: BTreeMap::new(),
            }),
            running: AtomicBool::new(true),
        }
    }

    async fn watch(&self, target: ListenTarget) -> FirestoreResult<()> {
        let target_id = target.target_id;
        let request = {
            let request = self.encode_watch_request(&target)?;
            serde_json::to_vec(&request)
                .map_err(|err| internal_error(format!("Failed to encode listen request: {err}")))?
        };

        let stream = {
            let mut guard = self.state.lock().await;
            guard.targets.insert(target_id, target);
            guard.stream.clone()
        };

        if let Some(stream) = stream {
            stream
                .send(request)
                .await
                .map_err(|err| internal_error(format!("Failed to send listen request: {err}")))?;
        }

        Ok(())
    }

    async fn unwatch(&self, target_id: i32) -> FirestoreResult<()> {
        let request = {
            let request = self.encode_unwatch_request(target_id);
            serde_json::to_vec(&request)
                .map_err(|err| internal_error(format!("Failed to encode unwatch request: {err}")))?
        };

        let stream = {
            let mut guard = self.state.lock().await;
            guard.targets.remove(&target_id);
            guard.stream.clone()
        };

        if let Some(stream) = stream {
            stream
                .send(request)
                .await
                .map_err(|err| internal_error(format!("Failed to send unwatch request: {err}")))?;
        }

        Ok(())
    }

    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn should_continue(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn encode_watch_request(&self, target: &ListenTarget) -> FirestoreResult<JsonValue> {
        let mut request = serde_json::Map::new();
        request.insert(
            "database".to_string(),
            JsonValue::String(self.serializer.database_name().to_string()),
        );

        let mut add_target = serde_json::Map::new();
        add_target.insert("targetId".to_string(), json!(target.target_id));
        if target.once {
            add_target.insert("once".to_string(), json!(true));
        }
        if let Some(token) = target.resume_token() {
            add_target.insert(
                "resumeToken".to_string(),
                json!(BASE64_STANDARD.encode(token)),
            );
        }
        match target.payload() {
            TargetPayload::Query {
                parent,
                structured_query,
            } => {
                let mut query = serde_json::Map::new();
                query.insert("parent".to_string(), json!(parent));
                query.insert("structuredQuery".to_string(), structured_query.clone());
                add_target.insert("query".to_string(), JsonValue::Object(query));
            }
            TargetPayload::Documents { documents } => {
                let doc_payload = json!({ "documents": documents });
                add_target.insert("documents".to_string(), doc_payload);
            }
        }

        request.insert("addTarget".to_string(), JsonValue::Object(add_target));
        if let Some(labels) = &target.labels {
            request.insert("labels".to_string(), json!(labels));
        }
        Ok(JsonValue::Object(request))
    }

    fn encode_unwatch_request(&self, target_id: i32) -> JsonValue {
        json!({
            "database": self.serializer.database_name(),
            "removeTarget": target_id
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<D> NetworkStreamHandler for ListenStreamHandler<D>
where
    D: ListenStreamDelegate,
{
    fn label(&self) -> &'static str {
        "listen"
    }

    fn should_continue(&self) -> bool {
        self.should_continue()
    }

    async fn on_open(
        &self,
        stream: Arc<dyn StreamHandle>,
        _credentials: StreamCredentials,
    ) -> FirestoreResult<()> {
        let targets = {
            let mut guard = self.state.lock().await;
            guard.stream = Some(Arc::clone(&stream));
            guard.targets.values().cloned().collect::<Vec<_>>()
        };

        for target in targets {
            let request = self.encode_watch_request(&target)?;
            let bytes = serde_json::to_vec(&request)
                .map_err(|err| internal_error(format!("Failed to encode listen request: {err}")))?;
            stream
                .send(bytes)
                .await
                .map_err(|err| internal_error(format!("Failed to send listen request: {err}")))?;
        }
        Ok(())
    }

    async fn on_message(&self, payload: Vec<u8>) -> FirestoreResult<()> {
        let value: JsonValue = serde_json::from_slice(&payload)
            .map_err(|err| internal_error(format!("Failed to decode listen response: {err}")))?;

        if let Some(change) = decode_watch_change(&self.serializer, &value)? {
            if let WatchChange::TargetChange(target_change) = &change {
                if let Some(token) = &target_change.resume_token {
                    let mut guard = self.state.lock().await;
                    if target_change.target_ids.is_empty() {
                        for target in guard.targets.values_mut() {
                            target.resume_token = Some(token.clone());
                        }
                    } else {
                        for target_id in &target_change.target_ids {
                            if let Some(target) = guard.targets.get_mut(target_id) {
                                target.resume_token = Some(token.clone());
                            }
                        }
                    }
                }
            }

            self.delegate.on_watch_change(change).await
        } else {
            Ok(())
        }
    }

    async fn on_close(&self) {
        let mut guard = self.state.lock().await;
        guard.stream = None;
    }

    async fn on_error(&self, error: FirestoreError) {
        self.delegate.on_stream_error(error).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::model::{DatabaseId, ResourcePath};
    use crate::firestore::remote::datastore::StreamingDatastoreImpl;
    use crate::firestore::remote::datastore::{NoopTokenProvider, TokenProviderArc};
    use crate::firestore::remote::stream::{InMemoryTransport, MultiplexedConnection};
    use crate::firestore::remote::watch_change::WatchChange;
    use crate::platform::runtime;
    use async_trait::async_trait;
    use std::time::Duration;

    #[derive(Default)]
    struct TestDelegate {
        responses: Mutex<Vec<WatchChange>>,
        errors: Mutex<Vec<FirestoreError>>,
    }

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl ListenStreamDelegate for TestDelegate {
        async fn on_watch_change(&self, change: WatchChange) -> FirestoreResult<()> {
            let mut guard = self.responses.lock().await;
            guard.push(change);
            Ok(())
        }

        async fn on_stream_error(&self, error: FirestoreError) {
            let mut guard = self.errors.lock().await;
            guard.push(error);
        }
    }

    fn serializer() -> JsonProtoSerializer {
        JsonProtoSerializer::new(DatabaseId::new("project", "(default)"))
    }

    fn query_definition() -> QueryDefinition {
        QueryDefinition {
            collection_path: ResourcePath::from_string("cities").unwrap(),
            parent_path: ResourcePath::root(),
            collection_id: "cities".to_string(),
            collection_group: None,
            filters: Vec::new(),
            request_order_by: Vec::new(),
            result_order_by: Vec::new(),
            limit: None,
            limit_type: crate::firestore::api::query::LimitType::First,
            request_start_at: None,
            request_end_at: None,
            result_start_at: None,
            result_end_at: None,
            projection: None,
        }
    }

    #[tokio::test]
    async fn listen_stream_replays_targets_on_open() {
        let (left_transport, right_transport) = InMemoryTransport::pair();
        let left_connection = Arc::new(MultiplexedConnection::new(left_transport));
        let right_connection = Arc::new(MultiplexedConnection::new(right_transport));
        let datastore = StreamingDatastoreImpl::new(Arc::clone(&left_connection));
        let datastore: Arc<dyn crate::firestore::remote::datastore::StreamingDatastore> =
            Arc::new(datastore);

        let auth_provider: TokenProviderArc = Arc::new(NoopTokenProvider::default());
        let layer = NetworkLayer::builder(datastore, auth_provider).build();

        let delegate = Arc::new(TestDelegate::default());
        let listen = ListenStream::new(layer, serializer(), delegate.clone());

        let target = ListenTarget::for_query(&serializer(), 1, &query_definition()).unwrap();
        listen.watch(target).await.expect("watch target");

        let peer_stream = right_connection.open_stream().await.expect("peer stream");
        let payload = peer_stream
            .next()
            .await
            .expect("handshake")
            .expect("payload");

        let json: JsonValue = serde_json::from_slice(&payload).expect("json");
        assert_eq!(
            json.get("database"),
            Some(&json!("projects/project/databases/(default)"))
        );
        assert!(json.get("addTarget").is_some());

        listen.stop();
    }

    use crate::firestore::remote::watch_change::TargetChangeState;

    #[tokio::test]
    async fn listen_stream_decodes_target_change() {
        let (left_transport, right_transport) = InMemoryTransport::pair();
        let left_connection = Arc::new(MultiplexedConnection::new(left_transport));
        let right_connection = Arc::new(MultiplexedConnection::new(right_transport));
        let datastore = StreamingDatastoreImpl::new(Arc::clone(&left_connection));
        let datastore: Arc<dyn crate::firestore::remote::datastore::StreamingDatastore> =
            Arc::new(datastore);

        let auth_provider: TokenProviderArc = Arc::new(NoopTokenProvider::default());
        let layer = NetworkLayer::builder(datastore, auth_provider).build();

        let delegate = Arc::new(TestDelegate::default());
        let listen = ListenStream::new(layer, serializer(), delegate.clone());
        let target = ListenTarget::for_query(&serializer(), 1, &query_definition()).unwrap();
        listen.watch(target).await.expect("watch target");

        let peer_stream = right_connection.open_stream().await.expect("peer stream");
        let _ = peer_stream
            .next()
            .await
            .expect("watch request")
            .expect("payload");

        let target_change = json!({
            "targetChange": {
                "targetIds": [1],
                "resumeToken": BASE64_STANDARD.encode(&[1, 2, 3]),
                "targetChangeType": "CURRENT"
            }
        });
        peer_stream
            .send(serde_json::to_vec(&target_change).unwrap())
            .await
            .expect("send target change");

        for _ in 0..10 {
            {
                let guard = delegate.responses.lock().await;
                if !guard.is_empty() {
                    break;
                }
            }
            runtime::sleep(Duration::from_millis(10)).await;
        }

        let responses = delegate.responses.lock().await;
        assert!(!responses.is_empty());
        match &responses[0] {
            WatchChange::TargetChange(change) => {
                assert_eq!(change.target_ids, vec![1]);
                assert_eq!(change.resume_token.as_deref(), Some(&[1, 2, 3][..]));
                assert_eq!(change.state, TargetChangeState::Current);
            }
            other => panic!("unexpected response: {other:?}"),
        }

        listen.stop();
    }
}
