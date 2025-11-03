use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::firestore::error::{FirestoreError, FirestoreResult};
use crate::firestore::remote::datastore::{
    RetrySettings, StreamHandle, StreamingDatastore, StreamingFuture,
};
use crate::platform::runtime;

#[derive(Clone, Copy, Debug)]
pub enum StreamKind {
    Listen,
    Write,
}

pub trait PersistentStreamDelegate: Send + Sync + 'static {
    fn stream_label(&self) -> &'static str;

    fn on_stream_open(
        &self,
        stream: Arc<dyn StreamHandle>,
    ) -> StreamingFuture<'_, FirestoreResult<()>>;

    fn on_stream_message(&self, message: Vec<u8>) -> StreamingFuture<'_, FirestoreResult<()>>;

    fn on_stream_close(&self) -> StreamingFuture<'_, ()>;

    fn on_stream_error(&self, error: FirestoreError) -> StreamingFuture<'_, ()>;

    fn should_continue(&self) -> bool;
}

pub struct PersistentStream<D>
where
    D: PersistentStreamDelegate,
{
    datastore: Arc<dyn StreamingDatastore>,
    delegate: Arc<D>,
    retry: RetrySettings,
    kind: StreamKind,
    running: Arc<AtomicBool>,
}

impl<D> PersistentStream<D>
where
    D: PersistentStreamDelegate,
{
    pub fn new(
        datastore: Arc<dyn StreamingDatastore>,
        delegate: Arc<D>,
        retry: RetrySettings,
        kind: StreamKind,
    ) -> Self {
        Self {
            datastore,
            delegate,
            retry,
            kind,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn start(self) -> PersistentStreamHandle {
        let running = Arc::clone(&self.running);
        runtime::spawn_detached(async move {
            self.run().await;
        });
        PersistentStreamHandle { running }
    }

    async fn run(self) {
        let label = self.delegate.stream_label();
        let mut backoff = BackoffState::new(self.retry.clone());

        while self.running.load(Ordering::SeqCst) && self.delegate.should_continue() {
            let open_result = match self.kind {
                StreamKind::Listen => self.datastore.open_listen_stream().await,
                StreamKind::Write => self.datastore.open_write_stream().await,
            };

            match open_result {
                Ok(stream) => {
                    if !self.running.load(Ordering::SeqCst) || !self.delegate.should_continue() {
                        let _ = stream.close().await;
                        break;
                    }

                    if let Err(err) = self.delegate.on_stream_open(Arc::clone(&stream)).await {
                        self.delegate.on_stream_error(err.clone()).await;
                        let _ = stream.close().await;
                        if let Some(delay) = backoff.next_delay() {
                            runtime::sleep(delay).await;
                        }
                        continue;
                    }

                    backoff.reset();
                    if !self.process_stream(stream).await {
                        break;
                    }
                }
                Err(err) => {
                    self.delegate.on_stream_error(err.clone()).await;
                    if let Some(delay) = backoff.next_delay() {
                        runtime::sleep(delay).await;
                    } else {
                        log::warn!("persistent stream {label} exhausted retries");
                        break;
                    }
                }
            }
        }

        let _ = self.delegate.on_stream_close().await;
    }

    async fn process_stream(&self, stream: Arc<dyn StreamHandle>) -> bool {
        loop {
            if !self.running.load(Ordering::SeqCst) || !self.delegate.should_continue() {
                let _ = stream.close().await;
                return false;
            }

            match stream.next().await {
                Some(Ok(payload)) => {
                    if let Err(err) = self.delegate.on_stream_message(payload).await {
                        self.delegate.on_stream_error(err.clone()).await;
                        let _ = stream.close().await;
                        return self.running.load(Ordering::SeqCst);
                    }
                }
                Some(Err(err)) => {
                    self.delegate.on_stream_error(err.clone()).await;
                    let _ = stream.close().await;
                    return self.running.load(Ordering::SeqCst);
                }
                None => {
                    let _ = self.delegate.on_stream_close().await;
                    return self.running.load(Ordering::SeqCst);
                }
            }
        }
    }
}

pub struct PersistentStreamHandle {
    running: Arc<AtomicBool>,
}

impl PersistentStreamHandle {
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

struct BackoffState {
    settings: RetrySettings,
    attempt: usize,
}

impl BackoffState {
    fn new(settings: RetrySettings) -> Self {
        Self {
            settings,
            attempt: 0,
        }
    }

    fn next_delay(&mut self) -> Option<Duration> {
        if self.settings.max_attempts > 0 && self.attempt >= self.settings.max_attempts {
            return None;
        }
        let delay = self
            .settings
            .initial_delay
            .mul_f64(self.settings.multiplier.powi(self.attempt as i32));
        let delay = delay.min(self.settings.max_delay);
        self.attempt += 1;
        Some(delay)
    }

    fn reset(&mut self) {
        self.attempt = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::remote::datastore::{
        streaming::box_stream_future, StreamingDatastore, StreamingDatastoreImpl,
    };
    use crate::firestore::remote::stream::{InMemoryTransport, MultiplexedConnection};
    use std::sync::Mutex;

    struct TestDelegate {
        messages: Arc<Mutex<Vec<Vec<u8>>>>,
        continue_flag: Arc<AtomicBool>,
    }

    impl TestDelegate {
        fn new() -> (Arc<Self>, Arc<AtomicBool>, Arc<Mutex<Vec<Vec<u8>>>>) {
            let messages = Arc::new(Mutex::new(Vec::new()));
            let continue_flag = Arc::new(AtomicBool::new(true));
            let delegate = Arc::new(Self {
                messages: Arc::clone(&messages),
                continue_flag: Arc::clone(&continue_flag),
            });
            (delegate, continue_flag, messages)
        }
    }

    impl PersistentStreamDelegate for TestDelegate {
        fn stream_label(&self) -> &'static str {
            "test"
        }

        fn on_stream_open(
            &self,
            _stream: Arc<dyn StreamHandle>,
        ) -> StreamingFuture<'_, FirestoreResult<()>> {
            box_stream_future(async { Ok(()) })
        }

        fn on_stream_message(&self, message: Vec<u8>) -> StreamingFuture<'_, FirestoreResult<()>> {
            let messages = Arc::clone(&self.messages);
            let flag = Arc::clone(&self.continue_flag);
            box_stream_future(async move {
                let mut guard = messages.lock().unwrap();
                guard.push(message);
                flag.store(false, Ordering::SeqCst);
                Ok(())
            })
        }

        fn on_stream_close(&self) -> StreamingFuture<'_, ()> {
            box_stream_future(async move {})
        }

        fn on_stream_error(&self, _error: FirestoreError) -> StreamingFuture<'_, ()> {
            box_stream_future(async move {})
        }

        fn should_continue(&self) -> bool {
            self.continue_flag.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn persistent_stream_receives_messages() {
        let (left_transport, right_transport) = InMemoryTransport::pair();
        let left_connection = Arc::new(MultiplexedConnection::new(left_transport));
        let right_connection = Arc::new(MultiplexedConnection::new(right_transport));
        let datastore = StreamingDatastoreImpl::new(Arc::clone(&left_connection));

        let (delegate, continue_flag, messages) = TestDelegate::new();
        let stream = PersistentStream::new(
            Arc::new(datastore) as Arc<dyn StreamingDatastore>,
            delegate,
            RetrySettings::default(),
            StreamKind::Listen,
        );
        let handle = stream.start();

        let peer_stream = right_connection.open_stream().await.expect("peer stream");
        peer_stream
            .send(b"hello".to_vec())
            .await
            .expect("send payload");
        peer_stream.close().await.expect("close peer");

        for _ in 0..10 {
            if !continue_flag.load(Ordering::SeqCst) {
                break;
            }
            runtime::sleep(Duration::from_millis(20)).await;
        }

        handle.stop();

        let guard = messages.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0], b"hello");
    }
}
