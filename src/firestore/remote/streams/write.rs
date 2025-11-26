use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_lock::Mutex;
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde_json::{json, Value as JsonValue};

use crate::firestore::error::{internal_error, invalid_argument, FirestoreError, FirestoreResult};
use crate::firestore::model::Timestamp;
use crate::firestore::remote::datastore::{StreamHandle, WriteOperation};
use crate::firestore::remote::network::{NetworkLayer, NetworkStreamHandler, StreamCredentials};
use crate::firestore::remote::serializer::JsonProtoSerializer;
use crate::firestore::remote::stream::persistent::PersistentStreamHandle;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait WriteStreamDelegate: Send + Sync + 'static {
    async fn on_handshake_complete(&self) -> FirestoreResult<()>;
    async fn on_write_response(&self, response: WriteResponse) -> FirestoreResult<()>;
    async fn on_stream_error(&self, error: FirestoreError);
}

pub struct WriteStream<D>
where
    D: WriteStreamDelegate,
{
    handler: Arc<WriteStreamHandler<D>>,
    handle: PersistentStreamHandle,
}

impl<D> WriteStream<D>
where
    D: WriteStreamDelegate,
{
    pub fn new(layer: NetworkLayer, serializer: JsonProtoSerializer, delegate: Arc<D>) -> Self {
        let handler = Arc::new(WriteStreamHandler::new(serializer, delegate));
        let handle = layer.write(Arc::clone(&handler));
        Self { handler, handle }
    }

    pub async fn write(&self, writes: Vec<WriteOperation>) -> FirestoreResult<()> {
        self.handler.write_mutations(writes).await
    }

    pub fn stop(&self) {
        self.handler.stop();
        self.handle.stop();
    }
}

struct WriteStreamHandler<D>
where
    D: WriteStreamDelegate,
{
    serializer: Arc<JsonProtoSerializer>,
    delegate: Arc<D>,
    state: Mutex<WriteStreamState>,
    running: AtomicBool,
}

struct WriteStreamState {
    stream: Option<Arc<dyn StreamHandle>>,
    handshake_complete: bool,
    last_stream_token: Option<Vec<u8>>,
}

impl<D> WriteStreamHandler<D>
where
    D: WriteStreamDelegate,
{
    fn new(serializer: JsonProtoSerializer, delegate: Arc<D>) -> Self {
        Self {
            serializer: Arc::new(serializer),
            delegate,
            state: Mutex::new(WriteStreamState {
                stream: None,
                handshake_complete: false,
                last_stream_token: None,
            }),
            running: AtomicBool::new(true),
        }
    }

    async fn write_mutations(&self, writes: Vec<WriteOperation>) -> FirestoreResult<()> {
        if writes.is_empty() {
            return Ok(());
        }

        let (stream, stream_token) = {
            let guard = self.state.lock().await;
            if !guard.handshake_complete {
                return Err(invalid_argument("Cannot write mutations before handshake completes"));
            }
            let stream = guard
                .stream
                .clone()
                .ok_or_else(|| internal_error("Write stream is not open"))?;
            let token = guard
                .last_stream_token
                .clone()
                .ok_or_else(|| internal_error("Missing stream token"))?;
            (stream, token)
        };

        let request = encode_write_request(&self.serializer, &stream_token, &writes)?;
        let bytes = serde_json::to_vec(&request)
            .map_err(|err| internal_error(format!("Failed to encode write request: {err}")))?;
        stream
            .send(bytes)
            .await
            .map_err(|err| internal_error(format!("Failed to send write request: {err}")))
    }

    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn should_continue(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    async fn send_handshake(&self, stream: Arc<dyn StreamHandle>) -> FirestoreResult<()> {
        let request = json!({
            "database": self.serializer.database_name()
        });
        let bytes =
            serde_json::to_vec(&request).map_err(|err| internal_error(format!("Failed to encode handshake: {err}")))?;
        stream
            .send(bytes)
            .await
            .map_err(|err| internal_error(format!("Failed to send handshake: {err}")))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<D> NetworkStreamHandler for WriteStreamHandler<D>
where
    D: WriteStreamDelegate,
{
    fn label(&self) -> &'static str {
        "write"
    }

    fn should_continue(&self) -> bool {
        self.should_continue()
    }

    async fn on_open(&self, stream: Arc<dyn StreamHandle>, _credentials: StreamCredentials) -> FirestoreResult<()> {
        {
            let mut guard = self.state.lock().await;
            guard.stream = Some(Arc::clone(&stream));
            guard.handshake_complete = false;
            guard.last_stream_token = None;
        }
        self.send_handshake(stream).await
    }

    async fn on_message(&self, payload: Vec<u8>) -> FirestoreResult<()> {
        let value: JsonValue = serde_json::from_slice(&payload)
            .map_err(|err| internal_error(format!("Failed to decode write response: {err}")))?;
        let response = decode_write_response(&self.serializer, &value)?;

        let (handshake_complete, delegate) = {
            let mut guard = self.state.lock().await;
            guard.last_stream_token = Some(response.stream_token.clone());
            if !guard.handshake_complete {
                guard.handshake_complete = true;
                (false, Arc::clone(&self.delegate))
            } else {
                (true, Arc::clone(&self.delegate))
            }
        };

        if !handshake_complete {
            delegate.on_handshake_complete().await
        } else {
            delegate.on_write_response(response).await
        }
    }

    async fn on_close(&self) {
        let mut guard = self.state.lock().await;
        guard.stream = None;
        guard.handshake_complete = false;
    }

    async fn on_error(&self, error: FirestoreError) {
        self.delegate.on_stream_error(error).await;
    }
}

#[derive(Debug, Clone)]
pub struct WriteResponse {
    pub stream_token: Vec<u8>,
    pub commit_time: Option<Timestamp>,
    pub write_results: Vec<WriteResult>,
}

#[derive(Debug, Clone)]
pub struct WriteResult {
    pub update_time: Option<Timestamp>,
    pub transform_results: Vec<crate::firestore::FirestoreValue>,
}

fn encode_write_request(
    serializer: &JsonProtoSerializer,
    stream_token: &[u8],
    writes: &[WriteOperation],
) -> FirestoreResult<JsonValue> {
    let encoded_writes: Vec<JsonValue> = writes
        .iter()
        .map(|write| serializer.encode_write_operation(write))
        .collect();
    Ok(json!({
        "database": serializer.database_name(),
        "streamToken": BASE64_STANDARD.encode(stream_token),
        "writes": encoded_writes
    }))
}

fn decode_write_response(serializer: &JsonProtoSerializer, value: &JsonValue) -> FirestoreResult<WriteResponse> {
    let stream_token_str = value
        .get("streamToken")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| internal_error("write response missing streamToken"))?;
    let stream_token = BASE64_STANDARD
        .decode(stream_token_str)
        .map_err(|err| internal_error(format!("Invalid streamToken: {err}")))?;

    let commit_time = value
        .get("commitTime")
        .and_then(JsonValue::as_str)
        .map(|timestamp| serializer.decode_timestamp_string(timestamp))
        .transpose()?;

    let write_results = value
        .get("writeResults")
        .and_then(JsonValue::as_array)
        .map(|results| {
            results
                .iter()
                .map(|entry| decode_write_result(serializer, entry))
                .collect::<FirestoreResult<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(WriteResponse {
        stream_token,
        commit_time,
        write_results,
    })
}

fn decode_write_result(serializer: &JsonProtoSerializer, value: &JsonValue) -> FirestoreResult<WriteResult> {
    let update_time = value
        .get("updateTime")
        .and_then(JsonValue::as_str)
        .map(|timestamp| serializer.decode_timestamp_string(timestamp))
        .transpose()?;

    let transform_results = value
        .get("transformResults")
        .and_then(JsonValue::as_array)
        .map(|results| {
            results
                .iter()
                .map(|entry| serializer.decode_value_json(entry))
                .collect::<FirestoreResult<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(WriteResult {
        update_time,
        transform_results,
    })
}

#[cfg(test)]
mod tests {
    use super::BASE64_STANDARD;
    use super::*;
    use crate::firestore::model::{DatabaseId, DocumentKey};
    use crate::firestore::remote::datastore::StreamingDatastoreImpl;
    use crate::firestore::remote::datastore::{NoopTokenProvider, TokenProviderArc};
    use crate::firestore::remote::stream::{InMemoryTransport, MultiplexedConnection};
    use crate::firestore::FirestoreValue;
    use crate::platform::runtime;
    use async_trait::async_trait;
    use serde_json::json;
    use serde_json::Value as JsonValue;
    use std::time::Duration;

    #[derive(Default)]
    struct TestDelegate {
        handshake_count: Mutex<usize>,
        responses: Mutex<Vec<WriteResponse>>,
        errors: Mutex<Vec<FirestoreError>>,
    }

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl WriteStreamDelegate for TestDelegate {
        async fn on_handshake_complete(&self) -> FirestoreResult<()> {
            let mut guard = self.handshake_count.lock().await;
            *guard += 1;
            Ok(())
        }

        async fn on_write_response(&self, response: WriteResponse) -> FirestoreResult<()> {
            let mut guard = self.responses.lock().await;
            guard.push(response);
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

    fn delete_operation() -> WriteOperation {
        let key = DocumentKey::from_string("cities/sf").unwrap();
        WriteOperation::Delete { key }
    }

    async fn spin_until<F, Fut>(mut condition: F)
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        for _ in 0..20 {
            if condition().await {
                break;
            }
            runtime::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn write_stream_sends_handshake_and_mutations() {
        let (left_transport, right_transport) = InMemoryTransport::pair();
        let left_connection = Arc::new(MultiplexedConnection::new(left_transport));
        let right_connection = Arc::new(MultiplexedConnection::new(right_transport));
        let datastore = StreamingDatastoreImpl::new(Arc::clone(&left_connection));
        let datastore: Arc<dyn crate::firestore::remote::datastore::StreamingDatastore> = Arc::new(datastore);

        let auth_provider: TokenProviderArc = Arc::new(NoopTokenProvider::default());
        let layer = NetworkLayer::builder(datastore, auth_provider).build();
        let delegate = Arc::new(TestDelegate::default());
        let stream_serializer = serializer();
        let write_stream = WriteStream::new(layer, stream_serializer.clone(), delegate.clone());

        let peer_stream = right_connection.open_stream().await.expect("peer stream");
        // Consume handshake request
        let handshake = peer_stream.next().await.expect("handshake frame").expect("payload");
        let request: JsonValue = serde_json::from_slice(&handshake).expect("json");
        assert_eq!(request.get("database"), Some(&json!("projects/project/databases/(default)")));

        // Send handshake response
        let handshake_response = json!({
            "streamToken": BASE64_STANDARD.encode([1u8, 2, 3]),
            "writeResults": [],
        });
        peer_stream
            .send(serde_json::to_vec(&handshake_response).unwrap())
            .await
            .expect("send handshake response");

        spin_until(|| async {
            let guard = delegate.handshake_count.lock().await;
            *guard > 0
        })
        .await;

        write_stream
            .write(vec![delete_operation()])
            .await
            .expect("write mutation");

        let write_request = peer_stream.next().await.expect("write frame").expect("payload");
        let request: JsonValue = serde_json::from_slice(&write_request).expect("json");
        assert!(request.get("streamToken").is_some());
        assert_eq!(
            request.get("writes").and_then(JsonValue::as_array).map(|arr| arr.len()),
            Some(1)
        );

        // Send write response
        let write_response = json!({
            "streamToken": BASE64_STANDARD.encode([4u8, 5, 6]),
            "commitTime": "2020-01-01T00:00:00Z",
            "writeResults": [
                {
                    "updateTime": "2020-01-01T00:00:00Z",
                    "transformResults": [
                        { "stringValue": "ok" }
                    ]
                }
            ]
        });
        peer_stream
            .send(serde_json::to_vec(&write_response).unwrap())
            .await
            .expect("send write response");

        spin_until(|| async {
            let guard = delegate.responses.lock().await;
            !guard.is_empty()
        })
        .await;

        let responses = delegate.responses.lock().await;
        assert_eq!(responses.len(), 1);
        let response = &responses[0];
        assert_eq!(response.stream_token, vec![4, 5, 6]);
        assert!(response.commit_time.is_some());
        assert_eq!(response.write_results.len(), 1);
        assert_eq!(
            response.write_results[0].transform_results[0],
            FirestoreValue::from_string("ok")
        );

        write_stream.stop();
    }
}
