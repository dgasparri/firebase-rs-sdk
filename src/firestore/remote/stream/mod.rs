use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_channel::{Receiver, Sender};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::firestore::error::{internal_error, FirestoreError, FirestoreResult};
use crate::platform::runtime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamId(u32);

impl StreamId {
    fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub enum FrameKind {
    Open,
    Data(Vec<u8>),
    Close,
    Error(FirestoreError),
}

#[derive(Clone, Debug)]
pub struct TransportFrame {
    stream_id: StreamId,
    kind: FrameKind,
}

impl TransportFrame {
    pub fn open(stream_id: StreamId) -> Self {
        Self {
            stream_id,
            kind: FrameKind::Open,
        }
    }

    pub fn data(stream_id: StreamId, payload: Vec<u8>) -> Self {
        Self {
            stream_id,
            kind: FrameKind::Data(payload),
        }
    }

    pub fn close(stream_id: StreamId) -> Self {
        Self {
            stream_id,
            kind: FrameKind::Close,
        }
    }

    pub fn error(stream_id: StreamId, error: FirestoreError) -> Self {
        Self {
            stream_id,
            kind: FrameKind::Error(error),
        }
    }

    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    pub fn kind(&self) -> &FrameKind {
        &self.kind
    }

    pub fn encode(&self) -> FirestoreResult<Vec<u8>> {
        let envelope = FrameEnvelope::try_from(self)?;
        serde_json::to_vec(&envelope)
            .map_err(|err| internal_error(format!("failed to encode transport frame: {err}")))
    }

    pub fn decode(bytes: &[u8]) -> FirestoreResult<Self> {
        let envelope: FrameEnvelope = serde_json::from_slice(bytes)
            .map_err(|err| internal_error(format!("failed to decode transport frame: {err}")))?;
        TransportFrame::try_from(envelope)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait StreamTransport: Send + Sync + 'static {
    async fn send(&self, frame: TransportFrame) -> FirestoreResult<()>;
    async fn next(&self) -> FirestoreResult<TransportFrame>;
}

pub struct MultiplexedConnection {
    transport: Arc<dyn StreamTransport>,
    next_stream_id: AtomicU32,
    outbound_tx: Sender<TransportFrame>,
    streams: Arc<Mutex<HashMap<StreamId, Sender<FrameKind>>>>,
}

impl MultiplexedConnection {
    pub fn new(transport: Arc<dyn StreamTransport>) -> Self {
        let (outbound_tx, outbound_rx) = async_channel::unbounded();
        let streams = Arc::new(Mutex::new(HashMap::new()));
        let manager = Self {
            transport: Arc::clone(&transport),
            next_stream_id: AtomicU32::new(1),
            outbound_tx,
            streams: Arc::clone(&streams),
        };

        manager.start_outbound_loop(outbound_rx);
        manager.start_inbound_loop(streams);
        manager
    }

    fn start_outbound_loop(&self, outbound_rx: Receiver<TransportFrame>) {
        let transport = Arc::clone(&self.transport);
        runtime::spawn_detached(async move {
            while let Ok(frame) = outbound_rx.recv().await {
                if let Err(err) = transport.send(frame).await {
                    log::warn!("multiplexed outbound loop terminated: {err:?}");
                    break;
                }
            }
        });
    }

    fn start_inbound_loop(&self, streams: Arc<Mutex<HashMap<StreamId, Sender<FrameKind>>>>) {
        let transport = Arc::clone(&self.transport);
        runtime::spawn_detached(async move {
            loop {
                match transport.next().await {
                    Ok(frame) => {
                        let stream_id = frame.stream_id();
                        let event = frame.kind().clone();
                        let maybe_sender = {
                            let guard = streams.lock().unwrap();
                            guard.get(&stream_id).cloned()
                        };
                        if let Some(sender) = maybe_sender {
                            if matches!(event, FrameKind::Close | FrameKind::Error(_)) {
                                let _ = sender.send(event).await;
                                let mut guard = streams.lock().unwrap();
                                guard.remove(&stream_id);
                            } else if let Err(err) = sender.send(event).await {
                                log::debug!(
                                    "dropping inbound frame for closed stream {}: {:?}",
                                    stream_id.value(),
                                    err
                                );
                            }
                        } else {
                            log::debug!("dropping frame for unknown stream {}", stream_id.value());
                        }
                    }
                    Err(err) => {
                        log::warn!("multiplexed inbound loop terminated: {err:?}");
                        break;
                    }
                }
            }
        });
    }

    pub async fn open_stream(&self) -> FirestoreResult<MultiplexedStream> {
        let stream_id = StreamId::new(self.next_stream_id.fetch_add(1, Ordering::SeqCst));
        let (inbound_tx, inbound_rx) = async_channel::unbounded();
        {
            let mut guard = self.streams.lock().unwrap();
            guard.insert(stream_id, inbound_tx);
        }
        self.outbound_tx
            .send(TransportFrame::open(stream_id))
            .await
            .map_err(|err| internal_error(format!("failed to queue open frame: {err}")))?;
        Ok(MultiplexedStream {
            id: stream_id,
            outbound: self.outbound_tx.clone(),
            inbound: inbound_rx,
            manager: self.clone_handle(),
        })
    }

    fn clone_handle(&self) -> MultiplexedConnectionHandle {
        MultiplexedConnectionHandle {
            outbound_tx: self.outbound_tx.clone(),
            streams: Arc::clone(&self.streams),
        }
    }
}

#[derive(Clone)]
pub struct MultiplexedConnectionHandle {
    outbound_tx: Sender<TransportFrame>,
    streams: Arc<Mutex<HashMap<StreamId, Sender<FrameKind>>>>,
}

impl MultiplexedConnectionHandle {
    pub fn close_stream(&self, stream_id: StreamId) {
        let _ = self.outbound_tx.try_send(TransportFrame::close(stream_id));
        let mut guard = self.streams.lock().unwrap();
        guard.remove(&stream_id);
    }
}

pub struct MultiplexedStream {
    id: StreamId,
    outbound: Sender<TransportFrame>,
    inbound: Receiver<FrameKind>,
    manager: MultiplexedConnectionHandle,
}

impl MultiplexedStream {
    pub fn id(&self) -> StreamId {
        self.id
    }

    pub async fn send(&self, payload: Vec<u8>) -> FirestoreResult<()> {
        self.outbound
            .send(TransportFrame::data(self.id, payload))
            .await
            .map_err(|err| internal_error(format!("failed to enqueue stream frame: {err}")))
    }

    pub async fn next(&self) -> Option<FirestoreResult<Vec<u8>>> {
        while let Ok(event) = self.inbound.recv().await {
            match event {
                FrameKind::Data(payload) => return Some(Ok(payload)),
                FrameKind::Close => return None,
                FrameKind::Error(err) => return Some(Err(err)),
                FrameKind::Open => continue,
            }
        }
        None
    }

    pub async fn close(&self) -> FirestoreResult<()> {
        self.outbound
            .send(TransportFrame::close(self.id))
            .await
            .map_err(|err| internal_error(format!("failed to enqueue close frame: {err}")))
    }
}

impl Drop for MultiplexedStream {
    fn drop(&mut self) {
        self.manager.close_stream(self.id);
    }
}

pub struct InMemoryTransport {
    inbound: Receiver<TransportFrame>,
    outbound: Sender<TransportFrame>,
}

impl InMemoryTransport {
    pub fn pair() -> (Arc<Self>, Arc<Self>) {
        let (left_tx, left_rx) = async_channel::unbounded();
        let (right_tx, right_rx) = async_channel::unbounded();

        let left = Arc::new(Self {
            inbound: left_rx,
            outbound: right_tx,
        });
        let right = Arc::new(Self {
            inbound: right_rx,
            outbound: left_tx,
        });
        (left, right)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl StreamTransport for InMemoryTransport {
    async fn send(&self, frame: TransportFrame) -> FirestoreResult<()> {
        self.outbound
            .send(frame)
            .await
            .map_err(|err| internal_error(format!("loopback transport send failed: {err}")))
    }

    async fn next(&self) -> FirestoreResult<TransportFrame> {
        self.inbound
            .recv()
            .await
            .map_err(|err| internal_error(format!("loopback transport recv failed: {err}")))
    }
}

#[derive(Serialize, Deserialize)]
struct FrameEnvelope {
    stream_id: u32,
    #[serde(flatten)]
    kind: FrameEnvelopeKind,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FrameEnvelopeKind {
    Open,
    Data { payload: Vec<u8> },
    Close,
}

impl TryFrom<&TransportFrame> for FrameEnvelope {
    type Error = FirestoreError;

    fn try_from(frame: &TransportFrame) -> Result<Self, Self::Error> {
        let kind = match frame.kind() {
            FrameKind::Open => FrameEnvelopeKind::Open,
            FrameKind::Data(payload) => FrameEnvelopeKind::Data {
                payload: payload.clone(),
            },
            FrameKind::Close => FrameEnvelopeKind::Close,
            FrameKind::Error(err) => {
                return Err(internal_error(format!(
                    "error frames cannot be serialized: {err}"
                )))
            }
        };

        Ok(Self {
            stream_id: frame.stream_id().value(),
            kind,
        })
    }
}

impl TryFrom<FrameEnvelope> for TransportFrame {
    type Error = FirestoreError;

    fn try_from(envelope: FrameEnvelope) -> Result<Self, Self::Error> {
        let stream_id = StreamId::new(envelope.stream_id);
        let kind = match envelope.kind {
            FrameEnvelopeKind::Open => FrameKind::Open,
            FrameEnvelopeKind::Data { payload } => FrameKind::Data(payload),
            FrameEnvelopeKind::Close => FrameKind::Close,
        };

        Ok(TransportFrame { stream_id, kind })
    }
}

#[cfg(not(target_arch = "wasm32"))]
type NativeWebSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[cfg(not(target_arch = "wasm32"))]
pub struct WebSocketTransport {
    inner: tokio::sync::Mutex<NativeWebSocket>,
}

#[cfg(not(target_arch = "wasm32"))]
impl WebSocketTransport {
    pub async fn connect(url: url::Url) -> FirestoreResult<Arc<Self>> {
        use tokio_tungstenite::connect_async;

        let (stream, _) = connect_async(url)
            .await
            .map_err(|err| internal_error(format!("websocket connect failed: {err}")))?;
        Ok(Arc::new(Self {
            inner: tokio::sync::Mutex::new(stream),
        }))
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl StreamTransport for WebSocketTransport {
    async fn send(&self, frame: TransportFrame) -> FirestoreResult<()> {
        use futures::SinkExt;
        use tokio_tungstenite::tungstenite::protocol::Message;

        let mut stream = self.inner.lock().await;
        let payload = frame.encode()?;
        stream
            .send(Message::Binary(payload))
            .await
            .map_err(|err| internal_error(format!("websocket send failed: {err}")))
    }

    async fn next(&self) -> FirestoreResult<TransportFrame> {
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::protocol::Message;

        loop {
            let maybe_msg = {
                let mut stream = self.inner.lock().await;
                stream.next().await
            };

            match maybe_msg {
                Some(Ok(Message::Binary(data))) => return TransportFrame::decode(&data),
                Some(Ok(Message::Text(text))) => return TransportFrame::decode(text.as_bytes()),
                Some(Ok(Message::Ping(payload))) => {
                    let mut stream = self.inner.lock().await;
                    if let Err(err) = stream.send(Message::Pong(payload)).await {
                        log::debug!("failed to reply to websocket ping: {err}");
                    }
                }
                Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Frame(_))) => {}
                Some(Ok(Message::Close(_))) => {
                    return Err(internal_error("websocket closed by peer"));
                }
                Some(Err(err)) => {
                    return Err(internal_error(format!("websocket receive failed: {err}")));
                }
                None => {
                    return Err(internal_error("websocket stream ended"));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stream_exchange_roundtrip() {
        let (left_transport, right_transport) = InMemoryTransport::pair();
        let left = MultiplexedConnection::new(left_transport);
        let right = MultiplexedConnection::new(right_transport);

        let left_stream = left.open_stream().await.expect("left stream");
        let right_stream = right.open_stream().await.expect("right stream");

        left_stream
            .send(b"hello".to_vec())
            .await
            .expect("left send");
        let payload = right_stream
            .next()
            .await
            .expect("right recv")
            .expect("payload");
        assert_eq!(payload, b"hello");

        right_stream
            .send(b"world".to_vec())
            .await
            .expect("right send");
        let payload = left_stream
            .next()
            .await
            .expect("left recv")
            .expect("payload");
        assert_eq!(payload, b"world");
    }

    #[tokio::test]
    async fn closing_stream_notifies_peer() {
        let (left_transport, right_transport) = InMemoryTransport::pair();
        let left = MultiplexedConnection::new(left_transport);
        let right = MultiplexedConnection::new(right_transport);

        let left_stream = left.open_stream().await.expect("left stream");
        let right_stream = right.open_stream().await.expect("right stream");

        left_stream.close().await.expect("left close");
        assert!(right_stream.next().await.is_none());
    }

    #[test]
    fn transport_frame_serialization_roundtrip() {
        let frame = TransportFrame::data(StreamId::new(7), b"payload".to_vec());
        let encoded = frame.encode().expect("encode");
        let decoded = TransportFrame::decode(&encoded).expect("decode");
        assert_eq!(decoded.stream_id().value(), 7);
        match decoded.kind() {
            FrameKind::Data(bytes) => assert_eq!(bytes, b"payload"),
            other => panic!("unexpected frame kind: {other:?}"),
        }
    }
}
