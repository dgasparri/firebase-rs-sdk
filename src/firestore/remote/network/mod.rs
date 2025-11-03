use std::sync::Arc;

use async_trait::async_trait;

use crate::firestore::error::{FirestoreError, FirestoreErrorCode, FirestoreResult};
use crate::firestore::remote::datastore::streaming::box_stream_future;
use crate::firestore::remote::datastore::{
    NoopTokenProvider, RetrySettings, StreamHandle, StreamingDatastore, TokenProviderArc,
};
use crate::firestore::remote::stream::{
    PersistentStream, PersistentStreamDelegate, PersistentStreamHandle, StreamKind,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamCredentials {
    pub auth_token: Option<String>,
    pub app_check_token: Option<String>,
    pub heartbeat_header: Option<String>,
}

#[derive(Clone)]
struct StreamCredentialProvider {
    auth_provider: TokenProviderArc,
    app_check_provider: TokenProviderArc,
    heartbeat_provider: Option<TokenProviderArc>,
}

impl StreamCredentialProvider {
    fn new(
        auth_provider: TokenProviderArc,
        app_check_provider: TokenProviderArc,
        heartbeat_provider: Option<TokenProviderArc>,
    ) -> Self {
        Self {
            auth_provider,
            app_check_provider,
            heartbeat_provider,
        }
    }

    async fn fetch(&self) -> FirestoreResult<StreamCredentials> {
        let auth_token = self.auth_provider.get_token().await?;
        let app_check_token = self.app_check_provider.get_token().await?;
        let heartbeat_header = match &self.heartbeat_provider {
            Some(provider) => provider.heartbeat_header().await?,
            None => None,
        };

        Ok(StreamCredentials {
            auth_token,
            app_check_token,
            heartbeat_header,
        })
    }

    fn invalidate(&self) {
        self.auth_provider.invalidate_token();
        self.app_check_provider.invalidate_token();
        if let Some(provider) = &self.heartbeat_provider {
            provider.invalidate_token();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait NetworkStreamHandler: Send + Sync + 'static {
    fn label(&self) -> &'static str;
    fn should_continue(&self) -> bool;

    async fn on_open(
        &self,
        stream: Arc<dyn StreamHandle>,
        credentials: StreamCredentials,
    ) -> FirestoreResult<()>;

    async fn on_message(&self, payload: Vec<u8>) -> FirestoreResult<()>;

    async fn on_close(&self);

    async fn on_error(&self, error: FirestoreError);
}

struct NetworkStreamDelegate<H>
where
    H: NetworkStreamHandler,
{
    handler: Arc<H>,
    credentials: StreamCredentialProvider,
}

impl<H> NetworkStreamDelegate<H>
where
    H: NetworkStreamHandler,
{
    fn new(handler: Arc<H>, credentials: StreamCredentialProvider) -> Self {
        Self {
            handler,
            credentials,
        }
    }
}

impl<H> PersistentStreamDelegate for NetworkStreamDelegate<H>
where
    H: NetworkStreamHandler,
{
    fn stream_label(&self) -> &'static str {
        self.handler.label()
    }

    fn on_stream_open(
        &self,
        stream: Arc<dyn StreamHandle>,
    ) -> crate::firestore::remote::datastore::StreamingFuture<'_, FirestoreResult<()>> {
        let handler = Arc::clone(&self.handler);
        let credentials = self.credentials.clone();
        box_stream_future(async move {
            let creds = credentials.fetch().await?;
            handler.on_open(stream, creds).await
        })
    }

    fn on_stream_message(
        &self,
        message: Vec<u8>,
    ) -> crate::firestore::remote::datastore::StreamingFuture<'_, FirestoreResult<()>> {
        let handler = Arc::clone(&self.handler);
        box_stream_future(async move { handler.on_message(message).await })
    }

    fn on_stream_close(&self) -> crate::firestore::remote::datastore::StreamingFuture<'_, ()> {
        let handler = Arc::clone(&self.handler);
        box_stream_future(async move { handler.on_close().await })
    }

    fn on_stream_error(
        &self,
        error: FirestoreError,
    ) -> crate::firestore::remote::datastore::StreamingFuture<'_, ()> {
        let handler = Arc::clone(&self.handler);
        let credentials = self.credentials.clone();
        box_stream_future(async move {
            if error.code == FirestoreErrorCode::Unauthenticated {
                credentials.invalidate();
            }
            handler.on_error(error).await;
        })
    }

    fn should_continue(&self) -> bool {
        self.handler.should_continue()
    }
}

#[derive(Clone)]
pub struct NetworkLayer {
    datastore: Arc<dyn StreamingDatastore>,
    credentials: StreamCredentialProvider,
    retry: RetrySettings,
}

impl NetworkLayer {
    pub fn builder(
        datastore: Arc<dyn StreamingDatastore>,
        auth_provider: TokenProviderArc,
    ) -> NetworkLayerBuilder {
        NetworkLayerBuilder::new(datastore, auth_provider)
    }

    pub fn listen<H>(&self, handler: Arc<H>) -> PersistentStreamHandle
    where
        H: NetworkStreamHandler,
    {
        self.spawn_stream(handler, StreamKind::Listen)
    }

    pub fn write<H>(&self, handler: Arc<H>) -> PersistentStreamHandle
    where
        H: NetworkStreamHandler,
    {
        self.spawn_stream(handler, StreamKind::Write)
    }

    fn spawn_stream<H>(&self, handler: Arc<H>, kind: StreamKind) -> PersistentStreamHandle
    where
        H: NetworkStreamHandler,
    {
        let delegate = Arc::new(NetworkStreamDelegate::new(
            Arc::clone(&handler),
            self.credentials.clone(),
        ));
        PersistentStream::new(
            Arc::clone(&self.datastore),
            delegate,
            self.retry.clone(),
            kind,
        )
        .start()
    }
}

pub struct NetworkLayerBuilder {
    datastore: Arc<dyn StreamingDatastore>,
    auth_provider: TokenProviderArc,
    app_check_provider: Option<TokenProviderArc>,
    heartbeat_provider: Option<TokenProviderArc>,
    retry: RetrySettings,
}

impl NetworkLayerBuilder {
    fn new(datastore: Arc<dyn StreamingDatastore>, auth_provider: TokenProviderArc) -> Self {
        Self {
            datastore,
            auth_provider,
            app_check_provider: None,
            heartbeat_provider: None,
            retry: RetrySettings::streaming_defaults(),
        }
    }

    pub fn with_app_check_provider(mut self, provider: TokenProviderArc) -> Self {
        self.app_check_provider = Some(provider);
        self
    }

    pub fn with_heartbeat_provider(mut self, provider: TokenProviderArc) -> Self {
        self.heartbeat_provider = Some(provider);
        self
    }

    pub fn with_retry(mut self, retry: RetrySettings) -> Self {
        self.retry = retry;
        self
    }

    pub fn build(self) -> NetworkLayer {
        let app_check = self
            .app_check_provider
            .unwrap_or_else(|| Arc::new(NoopTokenProvider::default()) as TokenProviderArc);
        let heartbeat_provider = self
            .heartbeat_provider
            .or_else(|| Some(Arc::clone(&app_check)));
        let credentials = StreamCredentialProvider::new(
            Arc::clone(&self.auth_provider),
            app_check,
            heartbeat_provider,
        );
        NetworkLayer {
            datastore: self.datastore,
            credentials,
            retry: self.retry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firestore::error::FirestoreResult;
    use crate::firestore::remote::datastore::streaming::StreamingDatastoreImpl;
    use crate::firestore::remote::stream::{InMemoryTransport, MultiplexedConnection};
    use crate::platform::runtime;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Clone)]
    struct TestTokenProvider {
        token: Option<String>,
        heartbeat: Option<String>,
        invalidated: Arc<AtomicBool>,
    }

    impl TestTokenProvider {
        fn new(token: Option<String>, heartbeat: Option<String>) -> (Self, Arc<AtomicBool>) {
            let invalidated = Arc::new(AtomicBool::new(false));
            (
                Self {
                    token,
                    heartbeat,
                    invalidated: Arc::clone(&invalidated),
                },
                invalidated,
            )
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl crate::firestore::remote::datastore::TokenProvider for TestTokenProvider {
        async fn get_token(&self) -> FirestoreResult<Option<String>> {
            Ok(self.token.clone())
        }

        fn invalidate_token(&self) {
            self.invalidated.store(true, Ordering::SeqCst);
        }

        async fn heartbeat_header(&self) -> FirestoreResult<Option<String>> {
            Ok(self.heartbeat.clone())
        }
    }

    struct TestHandler {
        credentials: Mutex<Vec<StreamCredentials>>,
        messages: Mutex<Vec<Vec<u8>>>,
        running: AtomicBool,
    }

    impl TestHandler {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                credentials: Mutex::new(Vec::new()),
                messages: Mutex::new(Vec::new()),
                running: AtomicBool::new(true),
            })
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl NetworkStreamHandler for TestHandler {
        fn label(&self) -> &'static str {
            "test-listen"
        }

        fn should_continue(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }

        async fn on_open(
            &self,
            stream: Arc<dyn StreamHandle>,
            credentials: StreamCredentials,
        ) -> FirestoreResult<()> {
            self.credentials.lock().unwrap().push(credentials);
            stream.send(b"handshake".to_vec()).await
        }

        async fn on_message(&self, payload: Vec<u8>) -> FirestoreResult<()> {
            self.messages.lock().unwrap().push(payload);
            self.running.store(false, Ordering::SeqCst);
            Ok(())
        }

        async fn on_close(&self) {
            self.running.store(false, Ordering::SeqCst);
        }

        async fn on_error(&self, _error: FirestoreError) {
            self.running.store(false, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn network_layer_stream_roundtrip() {
        let (left_transport, right_transport) = InMemoryTransport::pair();
        let left_connection = Arc::new(MultiplexedConnection::new(left_transport));
        let right_connection = Arc::new(MultiplexedConnection::new(right_transport));
        let datastore: Arc<dyn StreamingDatastore> =
            Arc::new(StreamingDatastoreImpl::new(Arc::clone(&left_connection)));

        let (auth_provider, auth_invalidated) = TestTokenProvider::new(Some("auth".into()), None);
        let (app_check_provider, _) =
            TestTokenProvider::new(Some("app-check".into()), Some("hb".into()));

        let layer = NetworkLayer::builder(datastore, Arc::new(auth_provider) as TokenProviderArc)
            .with_app_check_provider(Arc::new(app_check_provider) as TokenProviderArc)
            .build();

        let handler = TestHandler::new();
        let handle = layer.listen(Arc::clone(&handler));

        let peer_stream = right_connection.open_stream().await.expect("peer stream");

        let handshake = peer_stream
            .next()
            .await
            .expect("handshake frame")
            .expect("handshake payload");
        assert_eq!(handshake, b"handshake");

        peer_stream
            .send(b"payload".to_vec())
            .await
            .expect("send payload");
        peer_stream.close().await.expect("close peer stream");

        for _ in 0..10 {
            if !handler.should_continue() {
                break;
            }
            runtime::sleep(Duration::from_millis(20)).await;
        }

        handle.stop();

        assert!(!handler.should_continue());

        let credentials = handler.credentials.lock().unwrap();
        assert_eq!(credentials.len(), 1);
        let first = &credentials[0];
        assert_eq!(first.auth_token.as_deref(), Some("auth"));
        assert_eq!(first.app_check_token.as_deref(), Some("app-check"));
        assert_eq!(first.heartbeat_header.as_deref(), Some("hb"));

        let messages = handler.messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], b"payload");

        assert!(!auth_invalidated.load(Ordering::SeqCst));
    }
}
