use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::app::FirebaseApp;
use crate::database::error::{internal_error, DatabaseResult};

/// Describes a unique listener registration against the realtime backend.
///
/// The spec mirrors the JS `ListenSpec` shape produced in
/// `packages/database/src/core/PersistentConnection.ts`, but is simplified to
/// the parts required for our current transport scaffolding: a canonical path
/// plus pre-serialised REST-style query parameters used to scope the listen.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ListenSpec {
    path: Vec<String>,
    params: Vec<(String, String)>,
}

impl ListenSpec {
    pub fn new(mut path: Vec<String>, mut params: Vec<(String, String)>) -> Self {
        // Canonicalise both path and params so hashing/equality remain stable.
        // Paths are treated case-sensitive; params are sorted lexicographically
        // to avoid order-dependent duplication.
        //
        // The JS SDK derives listen IDs by serialising `query._queryObject`; we
        // opt for a cheaper canonicalisation until the richer SyncTree port
        // lands.
        params.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        path.iter_mut().for_each(|segment| {
            // Ensure no accidental leading/trailing whitespace sneaks in.
            *segment = segment.trim().to_owned();
        });
        Self { path, params }
    }

    fn path_string(&self) -> String {
        if self.path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.path.join("/"))
        }
    }

    fn params(&self) -> &[(String, String)] {
        &self.params
    }
}

#[async_trait(?Send)]
pub(crate) trait RealtimeTransport: Send + Sync {
    async fn connect(&self) -> DatabaseResult<()>;
    async fn disconnect(&self) -> DatabaseResult<()>;
    async fn listen(&self, spec: &ListenSpec) -> DatabaseResult<()>;
    async fn unlisten(&self, spec: &ListenSpec) -> DatabaseResult<()>;
}

#[derive(Debug, Default)]
enum RepoState {
    #[default]
    Offline,
    Online,
}

/// Minimal `Repo` port that tracks unique realtime listeners and dispatches
/// lifecycle events to the platform transport.
#[derive(Clone)]
pub(crate) struct Repo {
    transport: Arc<dyn RealtimeTransport>,
    state: Arc<Mutex<RepoState>>,
    active_listens: Arc<Mutex<HashMap<ListenSpec, usize>>>,
}

impl Repo {
    pub fn new_for_app(app: &FirebaseApp) -> Arc<Self> {
        Arc::new(Self {
            transport: select_transport(app),
            state: Arc::new(Mutex::new(RepoState::Offline)),
            active_listens: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn go_online(&self) -> DatabaseResult<()> {
        {
            let mut state = self.state.lock().unwrap();
            if matches!(*state, RepoState::Online) {
                return Ok(());
            }
            self.transport.connect().await?;
            *state = RepoState::Online;
        }
        Ok(())
    }

    pub async fn go_offline(&self) -> DatabaseResult<()> {
        {
            let mut state = self.state.lock().unwrap();
            if matches!(*state, RepoState::Offline) {
                return Ok(());
            }
            self.transport.disconnect().await?;
            *state = RepoState::Offline;
        }
        Ok(())
    }

    pub async fn listen(&self, spec: ListenSpec) -> DatabaseResult<()> {
        let should_issue_listen = {
            let mut listens = self.active_listens.lock().unwrap();
            let counter = listens.entry(spec.clone()).or_insert(0);
            let is_first = *counter == 0;
            *counter += 1;
            is_first
        };

        if should_issue_listen {
            if let Err(err) = self.transport.listen(&spec).await {
                // Roll back the reference count so later attempts can retry.
                let mut listens = self.active_listens.lock().unwrap();
                if let Some(count) = listens.get_mut(&spec) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        listens.remove(&spec);
                    }
                }
                return Err(err);
            }
        }

        Ok(())
    }

    pub async fn unlisten(&self, spec: ListenSpec) -> DatabaseResult<()> {
        let should_issue_unlisten = {
            let mut listens = self.active_listens.lock().unwrap();
            match listens.get_mut(&spec) {
                Some(counter) => {
                    *counter = counter.saturating_sub(1);
                    if *counter == 0 {
                        listens.remove(&spec);
                        true
                    } else {
                        false
                    }
                }
                None => false,
            }
        };

        if should_issue_unlisten {
            self.transport.unlisten(&spec).await?;
        }
        Ok(())
    }
}

fn select_transport(app: &FirebaseApp) -> Arc<dyn RealtimeTransport> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(transport) = native::websocket_transport(app) {
            return transport;
        }
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
    {
        if let Some(transport) = wasm::websocket_transport(app) {
            return transport;
        }
    }

    Arc::new(NoopTransport::default())
}

#[derive(Debug, Default)]
struct NoopTransport;

#[async_trait(?Send)]
impl RealtimeTransport for NoopTransport {
    async fn connect(&self) -> DatabaseResult<()> {
        Ok(())
    }

    async fn disconnect(&self) -> DatabaseResult<()> {
        Ok(())
    }

    async fn listen(&self, _spec: &ListenSpec) -> DatabaseResult<()> {
        Ok(())
    }

    async fn unlisten(&self, _spec: &ListenSpec) -> DatabaseResult<()> {
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::platform::runtime::spawn_detached;
    use crate::{
        app_check::{FirebaseAppCheckInternal, APP_CHECK_INTERNAL_COMPONENT_NAME},
        auth::Auth,
    };
    use futures::channel::oneshot;
    use futures_util::{SinkExt, StreamExt};
    use serde_json::{json, Map as JsonMap, Value as JsonValue};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::sync::Mutex as AsyncMutex;
    use tokio::task::JoinHandle;
    use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
    use url::Url;

    pub(super) fn websocket_transport(app: &FirebaseApp) -> Option<Arc<dyn RealtimeTransport>> {
        let url = app.options().database_url?;
        let parsed = Url::parse(&url).ok()?;
        let info = RepoInfo::from_url(parsed)?;
        Some(Arc::new(NativeWebSocketTransport::new(info, app.clone())))
    }

    type TcpWebSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
    type WebSocketSink = futures_util::stream::SplitSink<TcpWebSocket, Message>;

    #[derive(Clone, Debug)]
    struct RepoInfo {
        secure: bool,
        host: String,
        namespace: String,
    }

    impl RepoInfo {
        fn from_url(mut url: Url) -> Option<Self> {
            let secure = matches!(url.scheme(), "https" | "wss");
            let host = url.host_str()?.to_owned();
            let namespace = url
                .query_pairs()
                .find(|(key, _)| key == "ns")
                .map(|(_, value)| value.into_owned())
                .or_else(|| host.split('.').next().map(|segment| segment.to_owned()))?;
            // The Realtime Database requires paths to be empty for root listens.
            url.set_path("");
            Some(Self {
                secure,
                host,
                namespace,
            })
        }

        fn websocket_url(&self) -> Result<Url, url::ParseError> {
            let scheme = if self.secure { "wss" } else { "ws" };
            let mut url = Url::parse(&format!("{}://{}/.ws", scheme, self.host))?;
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("ns", &self.namespace);
                query.append_pair("v", "5");
            }
            Ok(url)
        }
    }

    #[derive(Debug)]
    struct NativeWebSocketTransport {
        repo_info: RepoInfo,
        state: Arc<NativeState>,
        app: FirebaseApp,
    }

    impl NativeWebSocketTransport {
        fn new(repo_info: RepoInfo, app: FirebaseApp) -> Self {
            Self {
                repo_info,
                state: Arc::new(NativeState::default()),
                app,
            }
        }

        async fn ensure_connection(&self) -> DatabaseResult<()> {
            {
                let guard = self.state.sink.lock().await;
                if guard.is_some() {
                    return Ok(());
                }
            }

            let (result_tx, result_rx) = oneshot::channel();
            let state = self.state.clone();
            let info = self.repo_info.clone();
            let app = self.app.clone();

            spawn_detached(async move {
                let result = connect_and_listen(state, info, app).await;
                let _ = result_tx.send(result);
            });

            result_rx
                .await
                .unwrap_or_else(|_| Err(internal_error("websocket connection task cancelled")))
        }

        async fn flush_pending(&self) -> DatabaseResult<()> {
            flush_pending_state(self.state.clone()).await
        }
    }

    #[derive(Debug, Default)]
    struct NativeState {
        sink: AsyncMutex<Option<WebSocketSink>>,
        reader: AsyncMutex<Option<JoinHandle<()>>>,
        pending: AsyncMutex<VecDeque<TransportCommand>>,
        next_request_id: AtomicU32,
    }

    async fn connect_and_listen(
        state: Arc<NativeState>,
        info: RepoInfo,
        app: FirebaseApp,
    ) -> DatabaseResult<()> {
        let url = info
            .websocket_url()
            .map_err(|err| internal_error(format!("invalid database_url for websocket: {err}")))?;

        let (stream, _response) = connect_async(url)
            .await
            .map_err(|err| internal_error(format!("failed to connect websocket: {err}")))?;
        let (sink, mut reader) = stream.split();

        {
            let mut guard = state.sink.lock().await;
            *guard = Some(sink);
        }

        let reader_state = state.clone();
        let reader_task: JoinHandle<()> = tokio::spawn(async move {
            while let Some(message) = reader.next().await {
                match message {
                    Ok(Message::Text(_)) | Ok(Message::Binary(_)) => {
                        // TODO(async-wasm): feed incoming messages into repo dispatch once
                        // the persistent connection protocol is ported.
                    }
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                        // Handled by tungstenite automatically; nothing to do until
                        // we expose connection-level metrics.
                    }
                    Ok(Message::Close(_)) | Err(_) => {
                        break;
                    }
                    _ => {}
                }
            }

            // Connection closed; clear sink so future listens can reconnect.
            {
                let mut guard = reader_state.sink.lock().await;
                guard.take();
            }

            {
                let mut guard = reader_state.reader.lock().await;
                guard.take();
            }
        });

        {
            let mut guard = state.reader.lock().await;
            if let Some(existing) = guard.replace(reader_task) {
                existing.abort();
            }
        }

        send_initial_tokens(state.clone(), app).await?;
        let _ = flush_pending_state(state.clone()).await;

        Ok(())
    }

    #[derive(Clone, Debug)]
    enum TransportCommand {
        Listen(ListenSpec),
        Unlisten(ListenSpec),
    }

    async fn flush_pending_state(state: Arc<NativeState>) -> DatabaseResult<()> {
        loop {
            let next_command = {
                let mut pending = state.pending.lock().await;
                pending.pop_front()
            };

            let Some(command) = next_command else {
                break;
            };

            let mut sink_guard = state.sink.lock().await;
            let Some(sink) = sink_guard.as_mut() else {
                // Connection dropped; re-queue and exit so the next
                // connection attempt can flush the backlog.
                let mut pending = state.pending.lock().await;
                pending.push_front(command);
                break;
            };

            let payload = serialize_command(state.as_ref(), &command)?;
            if let Err(err) = sink.send(Message::Text(payload)).await {
                let mut pending = state.pending.lock().await;
                pending.push_front(command);
                return Err(internal_error(format!(
                    "failed to send realtime command: {err}"
                )));
            }
        }

        Ok(())
    }

    #[async_trait(?Send)]
    impl RealtimeTransport for NativeWebSocketTransport {
        async fn connect(&self) -> DatabaseResult<()> {
            self.ensure_connection().await
        }

        async fn disconnect(&self) -> DatabaseResult<()> {
            let handle = {
                let mut guard = self.state.reader.lock().await;
                guard.take()
            };
            if let Some(handle) = handle {
                handle.abort();
            }

            let sink = {
                let mut guard = self.state.sink.lock().await;
                guard.take()
            };
            if let Some(mut sink) = sink {
                if let Err(err) = sink.close().await {
                    return Err(internal_error(format!("failed to close websocket: {err}")));
                }
            }

            Ok(())
        }

        async fn listen(&self, spec: &ListenSpec) -> DatabaseResult<()> {
            {
                let mut pending = self.state.pending.lock().await;
                pending.push_back(TransportCommand::Listen(spec.clone()));
            }
            self.ensure_connection().await?;
            self.flush_pending().await?;
            Ok(())
        }

        async fn unlisten(&self, spec: &ListenSpec) -> DatabaseResult<()> {
            {
                let mut pending = self.state.pending.lock().await;
                pending.push_back(TransportCommand::Unlisten(spec.clone()));
            }
            self.flush_pending().await?;
            Ok(())
        }
    }

    fn serialize_command(
        state: &NativeState,
        command: &TransportCommand,
    ) -> DatabaseResult<String> {
        let (action, spec) = match command {
            TransportCommand::Listen(spec) => ("listen", spec),
            TransportCommand::Unlisten(spec) => ("unlisten", spec),
        };

        let mut params = JsonMap::new();
        for (key, value) in spec.params() {
            params.insert(key.clone(), JsonValue::String(value.clone()));
        }

        let body = json!({
            "p": spec.path_string(),
            "q": JsonValue::Object(params.clone()),
            "h": "",
        });

        let request_id = next_request_id(state);
        let envelope = json!({
            "t": "d",
            "d": {
                "r": request_id,
                "a": action,
                "b": body,
            }
        });

        serde_json::to_string(&envelope)
            .map_err(|err| internal_error(format!("failed to encode realtime message: {err}")))
    }

    fn next_request_id(state: &NativeState) -> u32 {
        state.next_request_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    async fn send_initial_tokens(state: Arc<NativeState>, app: FirebaseApp) -> DatabaseResult<()> {
        if let Some(token) = fetch_auth_token(&app).await? {
            let body = json!({ "cred": token });
            send_request_message(&state, "auth", body).await?;
        }

        if let Some(token) = fetch_app_check_token(&app).await? {
            let body = json!({ "token": token });
            send_request_message(&state, "appcheck", body).await?;
        }

        Ok(())
    }

    async fn send_request_message(
        state: &Arc<NativeState>,
        action: &str,
        body: JsonValue,
    ) -> DatabaseResult<()> {
        let message = serialize_request(state.as_ref(), action, body)?;
        let mut guard = state.sink.lock().await;
        let Some(sink) = guard.as_mut() else {
            return Err(internal_error("websocket sink unavailable"));
        };
        sink.send(Message::Text(message))
            .await
            .map_err(|err| internal_error(format!("failed to send realtime request: {err}")))
    }

    fn serialize_request(
        state: &NativeState,
        action: &str,
        body: JsonValue,
    ) -> DatabaseResult<String> {
        let request_id = next_request_id(state);
        let envelope = json!({
            "t": "d",
            "d": {
                "r": request_id,
                "a": action,
                "b": body,
            }
        });

        serde_json::to_string(&envelope)
            .map_err(|err| internal_error(format!("failed to encode realtime request: {err}")))
    }

    async fn fetch_auth_token(app: &FirebaseApp) -> DatabaseResult<Option<String>> {
        let container = app.container();
        let auth_or_none = container
            .get_provider("auth-internal")
            .get_immediate_with_options::<Auth>(None, true)
            .map_err(|err| internal_error(format!("failed to resolve auth provider: {err}")))?;
        let auth = match auth_or_none {
            Some(auth) => Some(auth),
            None => container
                .get_provider("auth")
                .get_immediate_with_options::<Auth>(None, true)
                .map_err(|err| internal_error(format!("failed to resolve auth provider: {err}")))?,
        };
        let Some(auth) = auth else {
            return Ok(None);
        };

        match auth.get_token(false).await {
            Ok(Some(token)) if token.is_empty() => Ok(None),
            Ok(Some(token)) => Ok(Some(token)),
            Ok(None) => Ok(None),
            Err(err) => Err(internal_error(format!(
                "failed to obtain auth token: {err}"
            ))),
        }
    }

    async fn fetch_app_check_token(app: &FirebaseApp) -> DatabaseResult<Option<String>> {
        let container = app.container();
        let app_check = container
            .get_provider(APP_CHECK_INTERNAL_COMPONENT_NAME)
            .get_immediate_with_options::<FirebaseAppCheckInternal>(None, true)
            .map_err(|err| {
                internal_error(format!("failed to resolve app check provider: {err}"))
            })?;
        let Some(app_check) = app_check else {
            return Ok(None);
        };

        let result = app_check
            .get_token(false)
            .await
            .map_err(|err| internal_error(format!("failed to obtain App Check token: {err}")))?;
        if let Some(error) = result.error.or(result.internal_error) {
            return Err(internal_error(format!("App Check token error: {error}")));
        }
        if result.token.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result.token))
        }
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
mod wasm {
    use super::*;
    use wasm_bindgen::JsValue;
    use web_sys::Url;

    pub(super) fn websocket_transport(app: &FirebaseApp) -> Option<Arc<dyn RealtimeTransport>> {
        let url = app.options().database_url?;
        let parsed = Url::new(&url).ok()?;
        let info = RepoInfo::from_url(parsed)?;
        Some(Arc::new(WasmWebSocketTransport::new(info)))
    }

    #[derive(Clone, Debug)]
    struct RepoInfo {
        secure: bool,
        host: String,
        namespace: String,
    }

    impl RepoInfo {
        fn from_url(url: Url) -> Option<Self> {
            let secure = matches!(url.protocol().as_str(), "https:" | "wss:");
            let host = url.host()?.to_string();
            let namespace = url
                .search_params()
                .get("ns")
                .unwrap_or_else(|| host.split('.').next().unwrap_or("").to_string());
            if namespace.is_empty() {
                return None;
            }
            Some(Self {
                secure,
                host,
                namespace,
            })
        }

        fn websocket_url(&self) -> Result<String, JsValue> {
            let scheme = if self.secure { "wss" } else { "ws" };
            let url = format!("{}://{}/.ws?ns={}&v=5", scheme, self.host, self.namespace);
            Ok(url)
        }
    }

    #[derive(Debug)]
    struct WasmWebSocketTransport {
        repo_info: RepoInfo,
    }

    impl WasmWebSocketTransport {
        fn new(repo_info: RepoInfo) -> Self {
            Self { repo_info }
        }
    }

    #[async_trait(?Send)]
    impl RealtimeTransport for WasmWebSocketTransport {
        async fn connect(&self) -> DatabaseResult<()> {
            self.repo_info.websocket_url().map_err(|err| {
                internal_error(format!("invalid database_url for websocket: {err:?}"))
            })?;
            Ok(())
        }

        async fn disconnect(&self) -> DatabaseResult<()> {
            Ok(())
        }

        async fn listen(&self, _spec: &ListenSpec) -> DatabaseResult<()> {
            // TODO(async-wasm): Implement wasm WebSocket transport mirroring the
            // JS SDK `BrowserPollConnection` / `WebSocketConnection` stack.
            Ok(())
        }

        async fn unlisten(&self, _spec: &ListenSpec) -> DatabaseResult<()> {
            Ok(())
        }
    }
}
