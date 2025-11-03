use std::sync::Arc;

use super::{StreamHandle, StreamingDatastore, StreamingFuture};
use crate::firestore::error::FirestoreResult;
use crate::firestore::remote::stream::{MultiplexedConnection, MultiplexedStream};

use futures::FutureExt;

#[cfg(target_arch = "wasm32")]
pub(crate) fn box_stream_future<'a, F, T>(future: F) -> StreamingFuture<'a, T>
where
    F: std::future::Future<Output = T> + 'a,
{
    future.boxed_local()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn box_stream_future<'a, F, T>(future: F) -> StreamingFuture<'a, T>
where
    F: std::future::Future<Output = T> + Send + 'a,
{
    future.boxed()
}

pub struct StreamingDatastoreImpl {
    connection: Arc<MultiplexedConnection>,
}

impl StreamingDatastoreImpl {
    pub fn new(connection: Arc<MultiplexedConnection>) -> Self {
        Self { connection }
    }
}

impl StreamingDatastore for StreamingDatastoreImpl {
    fn open_listen_stream(&self) -> StreamingFuture<'_, FirestoreResult<Arc<dyn StreamHandle>>> {
        let connection = Arc::clone(&self.connection);
        box_stream_future(async move {
            let stream = connection.open_stream().await?;
            Ok(Arc::new(StreamingHandleImpl::new(stream)) as Arc<dyn StreamHandle>)
        })
    }

    fn open_write_stream(&self) -> StreamingFuture<'_, FirestoreResult<Arc<dyn StreamHandle>>> {
        let connection = Arc::clone(&self.connection);
        box_stream_future(async move {
            let stream = connection.open_stream().await?;
            Ok(Arc::new(StreamingHandleImpl::new(stream)) as Arc<dyn StreamHandle>)
        })
    }
}

pub struct StreamingHandleImpl {
    stream: MultiplexedStream,
}

impl StreamingHandleImpl {
    fn new(stream: MultiplexedStream) -> Self {
        Self { stream }
    }
}

impl StreamHandle for StreamingHandleImpl {
    fn send(&self, payload: Vec<u8>) -> StreamingFuture<'_, FirestoreResult<()>> {
        let stream = &self.stream;
        box_stream_future(async move { stream.send(payload).await })
    }

    fn next(&self) -> StreamingFuture<'_, Option<FirestoreResult<Vec<u8>>>> {
        let stream = &self.stream;
        box_stream_future(async move { stream.next().await })
    }

    fn close(&self) -> StreamingFuture<'_, FirestoreResult<()>> {
        let stream = &self.stream;
        box_stream_future(async move { stream.close().await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::remote::datastore::StreamingDatastore;
    use crate::firestore::remote::stream::InMemoryTransport;

    #[tokio::test]
    async fn datastore_stream_roundtrip() {
        let (left_transport, right_transport) = InMemoryTransport::pair();
        let left_connection = Arc::new(MultiplexedConnection::new(left_transport));
        let right_connection = Arc::new(MultiplexedConnection::new(right_transport));

        let datastore = StreamingDatastoreImpl::new(Arc::clone(&left_connection));
        let handle = datastore
            .open_listen_stream()
            .await
            .expect("open listen stream");

        let peer_stream = right_connection
            .open_stream()
            .await
            .expect("open peer stream");

        peer_stream
            .send(b"hello".to_vec())
            .await
            .expect("send payload");

        let payload = handle
            .next()
            .await
            .expect("receive event")
            .expect("payload");

        assert_eq!(payload, b"hello");
        handle.close().await.expect("close stream");
    }
}
