use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use futures::future::BoxFuture;
#[cfg(target_arch = "wasm32")]
use futures::future::LocalBoxFuture;

use crate::app::FirebaseApp;
use crate::app_check::{FirebaseAppCheckInternal, APP_CHECK_INTERNAL_COMPONENT_NAME};
use crate::auth::Auth;
use crate::database::error::{internal_error, DatabaseResult};
use reqwest::StatusCode;
use serde_json::Value as JsonValue;

#[cfg(not(target_arch = "wasm32"))]
type EventFuture = BoxFuture<'static, DatabaseResult<()>>;
#[cfg(target_arch = "wasm32")]
type EventFuture = LocalBoxFuture<'static, DatabaseResult<()>>;

#[cfg(not(target_arch = "wasm32"))]
type EventHandler = Arc<dyn Fn(String, serde_json::Value) -> EventFuture + Send + Sync>;
#[cfg(target_arch = "wasm32")]
type EventHandler = Arc<dyn Fn(String, serde_json::Value) -> EventFuture + Send + Sync>;

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

    #[allow(dead_code)]
    fn path_string(&self) -> String {
        if self.path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.path.join("/"))
        }
    }

    #[allow(dead_code)]
    fn params(&self) -> &[(String, String)] {
        &self.params
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub(crate) trait RealtimeTransport: Send + Sync {
    async fn connect(&self) -> DatabaseResult<()>;
    async fn disconnect(&self) -> DatabaseResult<()>;
    async fn listen(&self, spec: &ListenSpec) -> DatabaseResult<()>;
    async fn unlisten(&self, spec: &ListenSpec) -> DatabaseResult<()>;
    async fn on_disconnect(&self, request: OnDisconnectRequest) -> DatabaseResult<()>;
}

#[derive(Clone, Debug)]
pub(crate) struct OnDisconnectRequest {
    action: OnDisconnectAction,
    path: Vec<String>,
    payload: JsonValue,
}

impl OnDisconnectRequest {
    pub(crate) fn new(action: OnDisconnectAction, path: Vec<String>, payload: JsonValue) -> Self {
        Self {
            action,
            path,
            payload,
        }
    }

    fn into_inner(self) -> (OnDisconnectAction, Vec<String>, JsonValue) {
        (self.action, self.path, self.payload)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum OnDisconnectAction {
    Put,
    Merge,
    Cancel,
}

impl OnDisconnectAction {
    fn code(&self) -> &'static str {
        match self {
            OnDisconnectAction::Put => "o",
            OnDisconnectAction::Merge => "om",
            OnDisconnectAction::Cancel => "oc",
        }
    }
}

fn path_to_string(path: &[String]) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", path.join("/"))
    }
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
    event_handler: Arc<std::sync::Mutex<EventHandler>>,
}

impl Repo {
    pub fn new_for_app(app: &FirebaseApp) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            transport: select_transport(app, weak.clone()),
            state: Arc::new(Mutex::new(RepoState::Offline)),
            active_listens: Arc::new(Mutex::new(HashMap::new())),
            event_handler: Arc::new(std::sync::Mutex::new(default_event_handler())),
        })
    }

    #[cfg(test)]
    fn new_for_test(transport: Arc<dyn RealtimeTransport>) -> Arc<Self> {
        Arc::new(Self {
            transport,
            state: Arc::new(Mutex::new(RepoState::Offline)),
            active_listens: Arc::new(Mutex::new(HashMap::new())),
            event_handler: Arc::new(std::sync::Mutex::new(default_event_handler())),
        })
    }

    pub fn set_event_handler(&self, handler: EventHandler) {
        *self.event_handler.lock().unwrap() = handler;
    }

    pub async fn go_online(&self) -> DatabaseResult<()> {
        let should_connect = {
            let state = self.state.lock().unwrap();
            matches!(*state, RepoState::Offline)
        };

        if !should_connect {
            return Ok(());
        }

        self.transport.connect().await?;

        let mut state = self.state.lock().unwrap();
        *state = RepoState::Online;
        Ok(())
    }

    pub async fn go_offline(&self) -> DatabaseResult<()> {
        let should_disconnect = {
            let state = self.state.lock().unwrap();
            matches!(*state, RepoState::Online)
        };

        if !should_disconnect {
            return Ok(());
        }

        self.transport.disconnect().await?;

        let mut state = self.state.lock().unwrap();
        *state = RepoState::Offline;
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

    pub async fn on_disconnect_put(
        &self,
        path: Vec<String>,
        payload: JsonValue,
    ) -> DatabaseResult<()> {
        self.go_online().await?;
        self.transport
            .on_disconnect(OnDisconnectRequest::new(
                OnDisconnectAction::Put,
                path,
                payload,
            ))
            .await
    }

    pub async fn on_disconnect_merge(
        &self,
        path: Vec<String>,
        payload: JsonValue,
    ) -> DatabaseResult<()> {
        self.go_online().await?;
        self.transport
            .on_disconnect(OnDisconnectRequest::new(
                OnDisconnectAction::Merge,
                path,
                payload,
            ))
            .await
    }

    pub async fn on_disconnect_cancel(&self, path: Vec<String>) -> DatabaseResult<()> {
        self.go_online().await?;
        self.transport
            .on_disconnect(OnDisconnectRequest::new(
                OnDisconnectAction::Cancel,
                path,
                JsonValue::Null,
            ))
            .await
    }

    #[allow(dead_code)]
    pub(crate) async fn handle_action(
        &self,
        action: &str,
        body: &serde_json::Value,
    ) -> DatabaseResult<()> {
        let handler = self.event_handler.lock().unwrap().clone();
        handler(action.to_owned(), body.clone()).await
    }
}

fn default_event_handler() -> EventHandler {
    Arc::new(|_, _| -> EventFuture { Box::pin(async { Ok(()) }) })
}

fn select_transport(app: &FirebaseApp, repo: std::sync::Weak<Repo>) -> Arc<dyn RealtimeTransport> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(transport) = native::websocket_transport(app, repo.clone()) {
            return transport;
        }
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
    {
        if let Some(transport) = wasm::transport(app, repo.clone()) {
            return transport;
        }
    }

    Arc::new(NoopTransport::default())
}

#[derive(Debug, Default)]
struct NoopTransport;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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

    async fn on_disconnect(&self, _request: OnDisconnectRequest) -> DatabaseResult<()> {
        Err(internal_error(
            "Realtime transport is not available; onDisconnect cannot be scheduled",
        ))
    }
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
        .map_err(|err| internal_error(format!("failed to resolve app check provider: {err}")))?;
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

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::database::error::DatabaseError;
    use crate::platform::runtime::spawn_detached;
    use futures::channel::oneshot;
    use futures_util::{SinkExt, StreamExt};
    use serde_json::{json, Map as JsonMap, Value as JsonValue};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::LazyLock;
    use tokio::sync::Mutex as AsyncMutex;
    use tokio::task::JoinHandle;
    use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
    use url::Url;

    use crate::logger::Logger;

    static NATIVE_LOGGER: LazyLock<Logger> =
        LazyLock::new(|| Logger::new("@firebase/database/native_websocket"));

    use std::sync::{Mutex as StdMutex, Weak};

    pub(super) fn websocket_transport(
        app: &FirebaseApp,
        repo: Weak<Repo>,
    ) -> Option<Arc<dyn RealtimeTransport>> {
        let url = app.options().database_url?;
        let parsed = Url::parse(&url).ok()?;
        let info = RepoInfo::from_url(parsed)?;
        Some(Arc::new(NativeWebSocketTransport::new(
            info,
            app.clone(),
            repo,
        )))
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
        fn new(repo_info: RepoInfo, app: FirebaseApp, repo: Weak<Repo>) -> Self {
            Self {
                repo_info,
                state: Arc::new(NativeState::new(repo)),
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

    #[derive(Debug)]
    struct NativeState {
        sink: AsyncMutex<Option<WebSocketSink>>,
        reader: AsyncMutex<Option<JoinHandle<()>>>,
        pending: AsyncMutex<VecDeque<TransportCommand>>,
        next_request_id: AtomicU32,
        repo: StdMutex<Weak<Repo>>,
        pending_error: StdMutex<Option<DatabaseError>>,
    }

    impl NativeState {
        fn new(repo: Weak<Repo>) -> Self {
            Self {
                sink: AsyncMutex::new(None),
                reader: AsyncMutex::new(None),
                pending: AsyncMutex::new(VecDeque::new()),
                next_request_id: AtomicU32::new(0),
                repo: StdMutex::new(repo),
                pending_error: StdMutex::new(None),
            }
        }

        fn repo(&self) -> Option<Arc<Repo>> {
            self.repo.lock().unwrap().upgrade()
        }
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
                    Ok(Message::Text(payload)) => {
                        if let Err(err) = handle_incoming_message(&reader_state, payload).await {
                            NATIVE_LOGGER
                                .warn(format!("failed to process realtime message: {err}"));
                        }
                    }
                    Ok(Message::Binary(payload)) => {
                        if let Ok(text) = String::from_utf8(payload) {
                            if let Err(err) = handle_incoming_message(&reader_state, text).await {
                                NATIVE_LOGGER
                                    .warn(format!("failed to process realtime message: {err}"));
                            }
                        } else {
                            NATIVE_LOGGER.warn(
                                "received non-UTF8 binary realtime frame; dropping".to_string(),
                            );
                        }
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

            let pending_error = {
                let mut guard = reader_state.pending_error.lock().unwrap();
                guard.take()
            };

            if let Some(error) = pending_error {
                if let Some(repo) = reader_state.repo() {
                    if let Err(err) = repo
                        .handle_action("error", &JsonValue::String(error.to_string()))
                        .await
                    {
                        NATIVE_LOGGER.warn(format!("failed to propagate error to repo: {err}"));
                    }
                }
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

    async fn handle_incoming_message(state: &NativeState, payload: String) -> DatabaseResult<()> {
        let value: JsonValue = serde_json::from_str(&payload)
            .map_err(|err| internal_error(format!("failed to decode realtime message: {err}")))?;

        let Some(object) = value.as_object() else {
            return Ok(());
        };

        let Some(JsonValue::String(message_type)) = object.get("t") else {
            return Ok(());
        };

        match message_type.as_str() {
            "d" => handle_data_message(state, object.get("d")).await?,
            "c" => {
                NATIVE_LOGGER.debug(
                    "control message received; ignoring until protocol port completed".to_string(),
                );
            }
            _ => {
                NATIVE_LOGGER.debug(format!("unhandled realtime frame type '{message_type}'"));
            }
        }

        Ok(())
    }

    async fn handle_data_message(
        state: &NativeState,
        data: Option<&JsonValue>,
    ) -> DatabaseResult<()> {
        let Some(JsonValue::Object(data)) = data else {
            return Ok(());
        };

        if data.contains_key("r") {
            // Response frame; remove the pending handler once we track them.
            NATIVE_LOGGER.debug("realtime response received".to_string());
            return Ok(());
        }

        if let Some(action) = data.get("a").and_then(|value| value.as_str()) {
            if let Some(repo) = state.repo() {
                let body = data.get("b").cloned().unwrap_or(JsonValue::Null);
                if let Err(err) = repo.handle_action(action, &body).await {
                    NATIVE_LOGGER.warn(format!(
                        "failed to handle realtime action '{action}': {err}"
                    ));
                    *state.pending_error.lock().unwrap() = Some(err);
                }
            }
        }

        Ok(())
    }

    #[derive(Clone, Debug)]
    enum TransportCommand {
        Listen(ListenSpec),
        Unlisten(ListenSpec),
        OnDisconnect(OnDisconnectCommand),
    }

    #[derive(Clone, Debug)]
    struct OnDisconnectCommand {
        action: OnDisconnectAction,
        path: Vec<String>,
        payload: JsonValue,
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

    #[async_trait::async_trait]
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

        async fn on_disconnect(&self, request: OnDisconnectRequest) -> DatabaseResult<()> {
            let (action, path, payload) = request.into_inner();
            {
                let mut pending = self.state.pending.lock().await;
                pending.push_back(TransportCommand::OnDisconnect(OnDisconnectCommand {
                    action,
                    path,
                    payload,
                }));
            }
            self.ensure_connection().await?;
            self.flush_pending().await
        }
    }

    fn serialize_command(
        state: &NativeState,
        command: &TransportCommand,
    ) -> DatabaseResult<String> {
        match command {
            TransportCommand::Listen(spec) => serialize_listen(state, spec),
            TransportCommand::Unlisten(spec) => serialize_unlisten(state, spec),
            TransportCommand::OnDisconnect(command) => serialize_on_disconnect(state, command),
        }
    }

    fn serialize_listen(state: &NativeState, spec: &ListenSpec) -> DatabaseResult<String> {
        let mut params = JsonMap::new();
        for (key, value) in spec.params() {
            params.insert(key.clone(), JsonValue::String(value.clone()));
        }

        let body = json!({
            "p": spec.path_string(),
            "q": JsonValue::Object(params.clone()),
            "h": "",
        });

        serialize_request(state, "listen", body)
    }

    fn serialize_unlisten(state: &NativeState, spec: &ListenSpec) -> DatabaseResult<String> {
        let mut params = JsonMap::new();
        for (key, value) in spec.params() {
            params.insert(key.clone(), JsonValue::String(value.clone()));
        }

        let body = json!({
            "p": spec.path_string(),
            "q": JsonValue::Object(params.clone()),
            "h": "",
        });

        serialize_request(state, "unlisten", body)
    }

    fn serialize_on_disconnect(
        state: &NativeState,
        command: &OnDisconnectCommand,
    ) -> DatabaseResult<String> {
        let body = json!({
            "p": path_to_string(&command.path),
            "d": command.payload.clone(),
        });

        serialize_request(state, command.action.code(), body)
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
}

#[allow(dead_code)]
fn ensure_success(status: StatusCode, verb: &str) -> DatabaseResult<()> {
    if status.is_success() {
        Ok(())
    } else {
        Err(internal_error(format!(
            "onDisconnect {verb} request failed with status {status}"
        )))
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
mod wasm {
    use super::*;
    use crate::database::error::DatabaseError;
    use crate::logger::Logger;
    use async_lock::Mutex as AsyncMutex;
    use gloo_timers::future::TimeoutFuture;
    use js_sys::{ArrayBuffer, Uint8Array};
    use reqwest::{Client, StatusCode};
    use serde_json::{json, Map as JsonMap, Value as JsonValue};
    use std::collections::{HashMap, VecDeque};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, LazyLock, Mutex as StdMutex, Weak};
    use url::Url;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::spawn_local;
    use web_sys::{BinaryType, CloseEvent, Event, MessageEvent, WebSocket};

    const LONG_POLL_INTERVAL_MS: u32 = 1_500;
    const LONG_POLL_ERROR_BACKOFF_MS: u32 = 5_000;

    pub(super) fn transport(
        app: &FirebaseApp,
        repo: Weak<Repo>,
    ) -> Option<Arc<dyn RealtimeTransport>> {
        let url = app.options().database_url?;
        let parsed = Url::parse(&url).ok()?;
        let info = RepoInfo::from_url(parsed)?;
        Some(Arc::new(WasmRealtimeTransport::new(
            info,
            app.clone(),
            repo,
        )))
    }

    static WASM_LOGGER: LazyLock<Logger> =
        LazyLock::new(|| Logger::new("@firebase/database/wasm_websocket"));
    static WASM_LONG_POLL_LOGGER: LazyLock<Logger> =
        LazyLock::new(|| Logger::new("@firebase/database/wasm_long_poll"));

    #[derive(Clone, Debug)]
    struct RepoInfo {
        secure: bool,
        host: String,
        namespace: String,
    }

    impl RepoInfo {
        fn from_url(url: Url) -> Option<Self> {
            let secure = matches!(url.scheme(), "https" | "wss");
            let host = url.host_str()?.to_string();
            let namespace = url
                .query_pairs()
                .find(|(key, _)| key == "ns")
                .map(|(_, value)| value.into_owned())
                .or_else(|| host.split('.').next().map(|segment| segment.to_owned()))?;
            Some(Self {
                secure,
                host,
                namespace,
            })
        }

        fn websocket_url(&self) -> String {
            let scheme = if self.secure { "wss" } else { "ws" };
            format!("{}://{}/.ws?ns={}&v=5", scheme, self.host, self.namespace)
        }

        fn rest_url(&self, spec: &ListenSpec) -> DatabaseResult<Url> {
            let scheme = if self.secure { "https" } else { "http" };
            let base = format!("{}://{}", scheme, self.host);
            let mut url = Url::parse(&base)
                .map_err(|err| internal_error(format!("failed to parse database host: {err}")))?;
            let path = spec.path_string();
            if path == "/" {
                url.set_path(".json");
            } else {
                let trimmed = path.trim_start_matches('/');
                url.set_path(&format!("{}.json", trimmed));
            }
            {
                let mut pairs = url.query_pairs_mut();
                pairs.append_pair("ns", &self.namespace);
                for (key, value) in spec.params() {
                    pairs.append_pair(key, value);
                }
            }
            Ok(url)
        }

        fn rest_path(&self, path: &[String]) -> DatabaseResult<Url> {
            let scheme = if self.secure { "https" } else { "http" };
            let base = format!("{}://{}", scheme, self.host);
            let mut url = Url::parse(&base)
                .map_err(|err| internal_error(format!("failed to parse database host: {err}")))?;
            if path.is_empty() {
                url.set_path(".json");
            } else {
                url.set_path(&format!("{}.json", path.join("/")));
            }
            url.query_pairs_mut().append_pair("ns", &self.namespace);
            Ok(url)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ActiveTransport {
        WebSocket,
        LongPoll,
    }

    // Mirrors the JS `PersistentConnection` fallback logic, switching between
    // WebSocket and long-poll transports (`packages/database/src/core/PersistentConnection.ts`).
    #[derive(Debug)]
    struct WasmRealtimeTransport {
        websocket: Arc<WasmWebSocketTransport>,
        long_poll: Arc<WasmLongPollTransport>,
        active: AsyncMutex<ActiveTransport>,
    }

    impl WasmRealtimeTransport {
        fn new(repo_info: RepoInfo, app: FirebaseApp, repo: Weak<Repo>) -> Self {
            let websocket = Arc::new(WasmWebSocketTransport::new(
                repo_info.clone(),
                app.clone(),
                repo.clone(),
            ));
            let long_poll = Arc::new(WasmLongPollTransport::new(repo_info, app, repo));
            Self {
                websocket,
                long_poll,
                active: AsyncMutex::new(ActiveTransport::WebSocket),
            }
        }

        async fn set_active(&self, transport: ActiveTransport) {
            let mut guard = self.active.lock().await;
            *guard = transport;
        }

        async fn active(&self) -> ActiveTransport {
            *self.active.lock().await
        }
    }

    #[async_trait::async_trait(?Send)]
    impl RealtimeTransport for WasmRealtimeTransport {
        async fn connect(&self) -> DatabaseResult<()> {
            match self.websocket.connect().await {
                Ok(()) => {
                    self.set_active(ActiveTransport::WebSocket).await;
                    Ok(())
                }
                Err(err) => {
                    WASM_LOGGER.warn(format!(
                        "websocket connect failed; falling back to long-poll: {err}"
                    ));
                    self.long_poll.connect().await?;
                    self.set_active(ActiveTransport::LongPoll).await;
                    Ok(())
                }
            }
        }

        async fn disconnect(&self) -> DatabaseResult<()> {
            let result = match self.active().await {
                ActiveTransport::WebSocket => self.websocket.disconnect().await,
                ActiveTransport::LongPoll => self.long_poll.disconnect().await,
            };
            self.set_active(ActiveTransport::WebSocket).await;
            result
        }

        async fn listen(&self, spec: &ListenSpec) -> DatabaseResult<()> {
            match self.active().await {
                ActiveTransport::WebSocket => match self.websocket.listen(spec).await {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        WASM_LOGGER.warn(format!(
                            "websocket listen failed; switching to long-poll: {err}"
                        ));
                        let _ = self.websocket.disconnect().await;
                        self.long_poll.connect().await?;
                        self.long_poll.listen(spec).await?;
                        self.set_active(ActiveTransport::LongPoll).await;
                        Ok(())
                    }
                },
                ActiveTransport::LongPoll => self.long_poll.listen(spec).await,
            }
        }

        async fn unlisten(&self, spec: &ListenSpec) -> DatabaseResult<()> {
            match self.active().await {
                ActiveTransport::WebSocket => self.websocket.unlisten(spec).await,
                ActiveTransport::LongPoll => self.long_poll.unlisten(spec).await,
            }
        }

        async fn on_disconnect(&self, request: OnDisconnectRequest) -> DatabaseResult<()> {
            match self.active().await {
                ActiveTransport::WebSocket => self.websocket.on_disconnect(request).await,
                ActiveTransport::LongPoll => Err(internal_error(
                    "onDisconnect operations are not supported on the long-poll fallback yet",
                )),
            }
        }
    }

    #[derive(Debug)]
    struct WasmWebSocketTransport {
        repo_info: RepoInfo,
        app: FirebaseApp,
        state: Arc<WasmState>,
    }

    impl WasmWebSocketTransport {
        fn new(repo_info: RepoInfo, app: FirebaseApp, repo: Weak<Repo>) -> Self {
            Self {
                repo_info,
                app,
                state: Arc::new(WasmState::new(repo)),
            }
        }

        async fn ensure_connection(&self) -> DatabaseResult<()> {
            {
                let socket = self.state.socket.lock().await;
                if let Some(socket) = socket.as_ref() {
                    if socket.ready_state() == WebSocket::OPEN {
                        let _ = socket;
                        return self.flush_pending().await;
                    }
                }
            }
            self.connect_inner().await
        }

        async fn connect_inner(&self) -> DatabaseResult<()> {
            let url = self.repo_info.websocket_url();
            let socket = WebSocket::new(&url)
                .map_err(|err| internal_error(format!("failed to open websocket: {err:?}")))?;
            socket.set_binary_type(BinaryType::Arraybuffer);

            let state = self.state.clone();
            let app = self.app.clone();

            let on_open_state = state.clone();
            let on_open_app = app.clone();
            let on_open = Closure::wrap(Box::new(move |_event: Event| {
                let state = on_open_state.clone();
                let app = on_open_app.clone();
                spawn_local(async move {
                    if let Err(err) = send_initial_tokens(state.clone(), app).await {
                        *state.pending_error.lock().unwrap() = Some(err);
                    }
                    if let Err(err) = flush_pending_state(state.clone()).await {
                        *state.pending_error.lock().unwrap() = Some(err);
                    }
                });
            }) as Box<dyn FnMut(_)>);

            let on_message_state = state.clone();
            let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
                if let Some(text) = event.data().as_string() {
                    let state_for_task = on_message_state.clone();
                    spawn_local(async move {
                        if let Err(err) = handle_incoming_message(&state_for_task, text).await {
                            *state_for_task.pending_error.lock().unwrap() = Some(err);
                        }
                    });
                } else if let Ok(buffer) = event.data().dyn_into::<ArrayBuffer>() {
                    let array = Uint8Array::new(&buffer);
                    if let Ok(text) = std::str::from_utf8(&array.to_vec()) {
                        let state_for_task = on_message_state.clone();
                        let text_owned = text.to_string();
                        spawn_local(async move {
                            if let Err(err) =
                                handle_incoming_message(&state_for_task, text_owned).await
                            {
                                *state_for_task.pending_error.lock().unwrap() = Some(err);
                            }
                        });
                    }
                }
            }) as Box<dyn FnMut(_)>);

            let on_error_state = state.clone();
            let on_error = Closure::wrap(Box::new(move |_event: Event| {
                *on_error_state.pending_error.lock().unwrap() =
                    Some(internal_error("websocket error"));
            }) as Box<dyn FnMut(_)>);

            let on_close_state = state.clone();
            let on_close = Closure::wrap(Box::new(move |_event: CloseEvent| {
                let state = on_close_state.clone();
                spawn_local(async move {
                    state.socket.lock().await.take();
                    state.handles.lock().await.take();
                    let pending_error = {
                        let mut guard = state.pending_error.lock().unwrap();
                        guard.take()
                    };

                    if let Some(err) = pending_error {
                        if let Some(repo) = state.repo() {
                            if let Err(err) = repo
                                .handle_action(
                                    "error",
                                    &json!({
                                        "message": err.to_string()
                                    }),
                                )
                                .await
                            {
                                *state.pending_error.lock().unwrap() = Some(err);
                            }
                        }
                    }
                });
            }) as Box<dyn FnMut(_)>);

            socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
            socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
            socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            *self.state.socket.lock().await = Some(socket);
            *self.state.handles.lock().await = Some(WebSocketHandles {
                _on_open: on_open,
                _on_message: on_message,
                _on_error: on_error,
                _on_close: on_close,
            });

            Ok(())
        }

        async fn flush_pending(&self) -> DatabaseResult<()> {
            flush_pending_state(self.state.clone()).await
        }
    }

    #[async_trait::async_trait(?Send)]
    impl RealtimeTransport for WasmWebSocketTransport {
        async fn connect(&self) -> DatabaseResult<()> {
            self.ensure_connection().await
        }

        async fn disconnect(&self) -> DatabaseResult<()> {
            if let Some(socket) = self.state.socket.lock().await.take() {
                let _ = socket.close();
            }
            self.state.handles.lock().await.take();
            Ok(())
        }

        async fn listen(&self, spec: &ListenSpec) -> DatabaseResult<()> {
            {
                let mut pending = self.state.pending.lock().await;
                pending.push_back(TransportCommand::Listen(spec.clone()));
            }
            self.ensure_connection().await?;
            self.flush_pending().await
        }

        async fn unlisten(&self, spec: &ListenSpec) -> DatabaseResult<()> {
            {
                let mut pending = self.state.pending.lock().await;
                pending.push_back(TransportCommand::Unlisten(spec.clone()));
            }
            self.ensure_connection().await?;
            self.flush_pending().await
        }

        async fn on_disconnect(&self, request: OnDisconnectRequest) -> DatabaseResult<()> {
            let (action, path, payload) = request.into_inner();
            {
                let mut pending = self.state.pending.lock().await;
                pending.push_back(TransportCommand::OnDisconnect(OnDisconnectCommand {
                    action,
                    path,
                    payload,
                }));
            }
            self.ensure_connection().await?;
            self.flush_pending().await
        }
    }

    // Port of the browser long-poll transport from
    // `packages/database/src/realtime/BrowserPollConnection.ts` tailored for WASM.
    #[derive(Debug)]
    struct WasmLongPollTransport {
        repo_info: RepoInfo,
        app: FirebaseApp,
        client: Client,
        state: Arc<WasmLongPollState>,
    }

    impl WasmLongPollTransport {
        fn new(repo_info: RepoInfo, app: FirebaseApp, repo: Weak<Repo>) -> Self {
            Self {
                repo_info,
                app,
                client: Client::new(),
                state: Arc::new(WasmLongPollState::new(repo)),
            }
        }

        fn spawn_listener(&self, spec: ListenSpec, control: ListenerControl) {
            let state = self.state.clone();
            let info = self.repo_info.clone();
            let app = self.app.clone();
            let client = self.client.clone();
            spawn_local(async move {
                run_long_poll_loop(state, info, app, client, spec, control).await;
            });
        }

        async fn flush_on_disconnect(&self) -> DatabaseResult<()> {
            let commands = {
                let mut guard = self.state.pending_disconnect.lock().await;
                std::mem::take(&mut *guard)
            };

            for command in commands {
                match command.action {
                    OnDisconnectAction::Put => {
                        self.apply_put(&command.path, &command.payload).await?;
                        self.dispatch_local("d", &command.path, command.payload)
                            .await?;
                    }
                    OnDisconnectAction::Merge => {
                        self.apply_merge(&command.path, &command.payload).await?;
                        self.dispatch_local("m", &command.path, command.payload)
                            .await?;
                    }
                    OnDisconnectAction::Cancel => {}
                }
            }

            Ok(())
        }

        async fn apply_put(&self, path: &[String], payload: &JsonValue) -> DatabaseResult<()> {
            let mut url = self.repo_info.rest_path(path)?;
            if let Some(token) = fetch_auth_token(&self.app).await? {
                url.query_pairs_mut().append_pair("auth", &token);
            }

            let mut request = self.client.put(url);
            if let Some(token) = fetch_app_check_token(&self.app).await? {
                request = request.header("X-Firebase-AppCheck", token);
            }

            let response = request.json(payload).send().await.map_err(|err| {
                internal_error(format!("failed to apply onDisconnect PUT: {err}"))
            })?;

            ensure_success(response.status(), "PUT")
        }

        async fn apply_merge(&self, path: &[String], payload: &JsonValue) -> DatabaseResult<()> {
            let map = payload.as_object().ok_or_else(|| {
                internal_error("onDisconnect.update payload must be a JSON object".to_string())
            })?;

            let mut url = self.repo_info.rest_path(path)?;
            if let Some(token) = fetch_auth_token(&self.app).await? {
                url.query_pairs_mut().append_pair("auth", &token);
            }

            let mut request = self.client.patch(url);
            if let Some(token) = fetch_app_check_token(&self.app).await? {
                request = request.header("X-Firebase-AppCheck", token);
            }

            let response = request.json(map).send().await.map_err(|err| {
                internal_error(format!("failed to apply onDisconnect PATCH: {err}"))
            })?;

            ensure_success(response.status(), "PATCH")
        }

        async fn dispatch_local(
            &self,
            action: &str,
            path: &[String],
            payload: JsonValue,
        ) -> DatabaseResult<()> {
            if let Some(repo) = self.state.repo() {
                let body = json!({
                    "p": path_to_string(path),
                    "d": payload,
                });
                repo.handle_action(action, &body).await
            } else {
                Ok(())
            }
        }
    }

    #[async_trait::async_trait(?Send)]
    impl RealtimeTransport for WasmLongPollTransport {
        async fn connect(&self) -> DatabaseResult<()> {
            self.state.online.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn disconnect(&self) -> DatabaseResult<()> {
            self.state.online.store(false, Ordering::SeqCst);
            let handles = {
                let mut listeners = self.state.listeners.lock().await;
                listeners.drain().collect::<Vec<_>>()
            };
            for (_, control) in handles {
                control.cancel();
            }
            self.flush_on_disconnect().await?;
            Ok(())
        }

        async fn listen(&self, spec: &ListenSpec) -> DatabaseResult<()> {
            let control = {
                let mut listeners = self.state.listeners.lock().await;
                if let Some(existing) = listeners.get(spec) {
                    existing.resume();
                    return Ok(());
                }
                let control = ListenerControl::new();
                listeners.insert(spec.clone(), control.clone());
                control
            };
            self.spawn_listener(spec.clone(), control);
            Ok(())
        }

        async fn unlisten(&self, spec: &ListenSpec) -> DatabaseResult<()> {
            let control = {
                let mut listeners = self.state.listeners.lock().await;
                listeners.remove(spec)
            };
            if let Some(control) = control {
                control.cancel();
            }
            Ok(())
        }

        async fn on_disconnect(&self, request: OnDisconnectRequest) -> DatabaseResult<()> {
            let OnDisconnectRequest {
                action,
                path,
                payload,
            } = request;
            let mut pending = self.state.pending_disconnect.lock().await;
            match action {
                OnDisconnectAction::Cancel => {
                    pending.retain(|existing| existing.path != path);
                }
                OnDisconnectAction::Put | OnDisconnectAction::Merge => {
                    pending.retain(|existing| existing.path != path);
                    pending.push(OnDisconnectCommand {
                        action,
                        path,
                        payload,
                    });
                }
            }
            Ok(())
        }
    }

    #[derive(Debug)]
    struct WasmLongPollState {
        repo: StdMutex<Weak<Repo>>,
        listeners: AsyncMutex<HashMap<ListenSpec, ListenerControl>>,
        online: AtomicBool,
        pending_disconnect: AsyncMutex<Vec<OnDisconnectCommand>>,
    }

    impl WasmLongPollState {
        fn new(repo: Weak<Repo>) -> Self {
            Self {
                repo: StdMutex::new(repo),
                listeners: AsyncMutex::new(HashMap::new()),
                online: AtomicBool::new(false),
                pending_disconnect: AsyncMutex::new(Vec::new()),
            }
        }

        fn repo(&self) -> Option<Arc<Repo>> {
            self.repo.lock().unwrap().upgrade()
        }
    }

    #[derive(Clone, Debug)]
    struct ListenerControl {
        cancel_flag: Arc<AtomicBool>,
        last_value: Arc<AsyncMutex<Option<JsonValue>>>,
        etag: Arc<AsyncMutex<Option<String>>>,
    }

    impl ListenerControl {
        fn new() -> Self {
            Self {
                cancel_flag: Arc::new(AtomicBool::new(false)),
                last_value: Arc::new(AsyncMutex::new(None)),
                etag: Arc::new(AsyncMutex::new(None)),
            }
        }

        fn cancel(&self) {
            self.cancel_flag.store(true, Ordering::SeqCst);
        }

        fn resume(&self) {
            self.cancel_flag.store(false, Ordering::SeqCst);
        }

        fn is_cancelled(&self) -> bool {
            self.cancel_flag.load(Ordering::SeqCst)
        }

        async fn current_etag(&self) -> Option<String> {
            self.etag.lock().await.clone()
        }

        async fn set_etag(&self, value: Option<String>) {
            *self.etag.lock().await = value;
        }

        async fn mark_published(&self, value: &JsonValue) -> bool {
            let mut guard = self.last_value.lock().await;
            if let Some(existing) = guard.as_ref() {
                if existing == value {
                    return false;
                }
            }
            *guard = Some(value.clone());
            true
        }
    }

    async fn run_long_poll_loop(
        state: Arc<WasmLongPollState>,
        repo_info: RepoInfo,
        app: FirebaseApp,
        client: Client,
        spec: ListenSpec,
        control: ListenerControl,
    ) {
        while !control.is_cancelled() {
            if !state.online.load(Ordering::SeqCst) {
                TimeoutFuture::new(LONG_POLL_INTERVAL_MS).await;
                continue;
            }

            match poll_once(&repo_info, &app, &client, &control, &spec).await {
                Ok(Some(value)) => {
                    if let Err(err) = deliver_snapshot(&state, &spec, &control, value).await {
                        WASM_LONG_POLL_LOGGER
                            .warn(format!("failed to deliver long-poll snapshot: {err}"));
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    WASM_LONG_POLL_LOGGER.warn(format!("long-poll request failed: {err}"));
                    propagate_error(&state, err.to_string()).await;
                    TimeoutFuture::new(LONG_POLL_ERROR_BACKOFF_MS).await;
                    continue;
                }
            }

            TimeoutFuture::new(LONG_POLL_INTERVAL_MS).await;
        }
    }

    async fn poll_once(
        repo_info: &RepoInfo,
        app: &FirebaseApp,
        client: &Client,
        control: &ListenerControl,
        spec: &ListenSpec,
    ) -> DatabaseResult<Option<JsonValue>> {
        let mut url = repo_info.rest_url(spec)?;

        if let Some(token) = fetch_auth_token(app).await? {
            {
                let mut pairs = url.query_pairs_mut();
                pairs.append_pair("auth", &token);
            }
        }

        let mut request = client.get(url.as_str());
        request = request.header("X-Firebase-ETag", "true");
        if let Some(etag) = control.current_etag().await {
            request = request.header("If-None-Match", etag);
        }

        if let Some(token) = fetch_app_check_token(app).await? {
            request = request.header("X-Firebase-AppCheck", token);
        }

        let response = request
            .send()
            .await
            .map_err(|err| internal_error(format!("long-poll request failed: {err}")))?;

        let status = response.status();
        if status == StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(internal_error(format!(
                "long-poll request failed with status {status}"
            )));
        }

        if let Some(etag) = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
        {
            control.set_etag(Some(etag.to_string())).await;
        }

        let payload = response
            .json::<JsonValue>()
            .await
            .map_err(|err| internal_error(format!("failed to decode long-poll payload: {err}")))?;
        Ok(Some(payload))
    }

    async fn deliver_snapshot(
        state: &Arc<WasmLongPollState>,
        spec: &ListenSpec,
        control: &ListenerControl,
        value: JsonValue,
    ) -> DatabaseResult<()> {
        if !control.mark_published(&value).await {
            return Ok(());
        }

        if let Some(repo) = state.repo() {
            let body = json!({ "p": spec.path_string(), "d": value });
            repo.handle_action("d", &body).await
        } else {
            Ok(())
        }
    }

    async fn propagate_error(state: &Arc<WasmLongPollState>, message: String) {
        if let Some(repo) = state.repo() {
            if let Err(err) = repo
                .handle_action("error", &JsonValue::String(message.clone()))
                .await
            {
                WASM_LONG_POLL_LOGGER.warn(format!("failed to propagate long-poll error: {err}"));
            }
        }
    }

    #[derive(Debug)]
    struct WebSocketHandles {
        _on_open: Closure<dyn FnMut(Event)>,
        _on_message: Closure<dyn FnMut(MessageEvent)>,
        _on_error: Closure<dyn FnMut(Event)>,
        _on_close: Closure<dyn FnMut(CloseEvent)>,
    }

    #[derive(Clone, Debug)]
    enum TransportCommand {
        Listen(ListenSpec),
        Unlisten(ListenSpec),
        OnDisconnect(OnDisconnectCommand),
    }

    #[derive(Clone, Debug)]
    struct OnDisconnectCommand {
        action: OnDisconnectAction,
        path: Vec<String>,
        payload: JsonValue,
    }

    #[derive(Debug)]
    struct WasmState {
        socket: AsyncMutex<Option<WebSocket>>,
        pending: AsyncMutex<VecDeque<TransportCommand>>,
        next_request_id: AtomicU32,
        repo: StdMutex<Weak<Repo>>,
        handles: AsyncMutex<Option<WebSocketHandles>>,
        pending_error: StdMutex<Option<DatabaseError>>,
    }

    impl WasmState {
        fn new(repo: Weak<Repo>) -> Self {
            Self {
                socket: AsyncMutex::new(None),
                pending: AsyncMutex::new(VecDeque::new()),
                next_request_id: AtomicU32::new(0),
                repo: StdMutex::new(repo),
                handles: AsyncMutex::new(None),
                pending_error: StdMutex::new(None),
            }
        }

        fn repo(&self) -> Option<Arc<Repo>> {
            self.repo.lock().unwrap().upgrade()
        }
    }

    unsafe impl Send for WasmState {}
    unsafe impl Sync for WasmState {}
    unsafe impl Send for WasmWebSocketTransport {}
    unsafe impl Sync for WasmWebSocketTransport {}

    async fn handle_incoming_message(state: &WasmState, payload: String) -> DatabaseResult<()> {
        let value: JsonValue = serde_json::from_str(&payload)
            .map_err(|err| internal_error(format!("failed to decode realtime message: {err}")))?;

        let Some(object) = value.as_object() else {
            return Ok(());
        };

        let Some(JsonValue::String(message_type)) = object.get("t") else {
            return Ok(());
        };

        match message_type.as_str() {
            "d" => handle_data_message(state, object.get("d")).await?,
            "c" => {
                WASM_LOGGER.debug(
                    "control message received; ignoring until protocol port completed".to_string(),
                );
            }
            _ => {
                WASM_LOGGER.debug(format!("unhandled realtime frame type '{message_type}'"));
            }
        }

        Ok(())
    }

    async fn handle_data_message(
        state: &WasmState,
        data: Option<&JsonValue>,
    ) -> DatabaseResult<()> {
        let Some(JsonValue::Object(data)) = data else {
            return Ok(());
        };

        if data.contains_key("r") {
            WASM_LOGGER.debug("realtime response received".to_string());
            return Ok(());
        }

        if let Some(action) = data.get("a").and_then(|value| value.as_str()) {
            if let Some(repo) = state.repo() {
                let body = data.get("b").cloned().unwrap_or(JsonValue::Null);
                if let Err(err) = repo.handle_action(action, &body).await {
                    WASM_LOGGER.warn(format!(
                        "failed to handle realtime action '{action}': {err}"
                    ));
                    *state.pending_error.lock().unwrap() = Some(err);
                }
            }
        }

        Ok(())
    }

    async fn flush_pending_state(state: Arc<WasmState>) -> DatabaseResult<()> {
        loop {
            let command = {
                let mut pending = state.pending.lock().await;
                pending.pop_front()
            };

            let Some(command) = command else {
                break;
            };

            let payload = serialize_command(state.as_ref(), &command)?;

            let socket_guard = state.socket.lock().await;
            let Some(socket) = socket_guard.as_ref() else {
                let mut pending = state.pending.lock().await;
                pending.push_front(command);
                break;
            };

            if socket.ready_state() != WebSocket::OPEN {
                let mut pending = state.pending.lock().await;
                pending.push_front(command);
                break;
            }

            if let Err(err) = socket.send_with_str(&payload) {
                let mut pending = state.pending.lock().await;
                pending.push_front(command);
                return Err(internal_error(format!(
                    "failed to send realtime command: {err:?}"
                )));
            }
        }
        Ok(())
    }

    fn serialize_command(state: &WasmState, command: &TransportCommand) -> DatabaseResult<String> {
        match command {
            TransportCommand::Listen(spec) => serialize_listen(state, spec),
            TransportCommand::Unlisten(spec) => serialize_unlisten(state, spec),
            TransportCommand::OnDisconnect(command) => serialize_on_disconnect(state, command),
        }
    }

    fn serialize_listen(state: &WasmState, spec: &ListenSpec) -> DatabaseResult<String> {
        let mut params = JsonMap::new();
        for (key, value) in spec.params() {
            params.insert(key.clone(), JsonValue::String(value.clone()));
        }

        let body = json!({
            "p": spec.path_string(),
            "q": JsonValue::Object(params),
            "h": "",
        });

        serialize_request(state, "listen", body)
    }

    fn serialize_unlisten(state: &WasmState, spec: &ListenSpec) -> DatabaseResult<String> {
        let mut params = JsonMap::new();
        for (key, value) in spec.params() {
            params.insert(key.clone(), JsonValue::String(value.clone()));
        }

        let body = json!({
            "p": spec.path_string(),
            "q": JsonValue::Object(params),
            "h": "",
        });

        serialize_request(state, "unlisten", body)
    }

    fn serialize_on_disconnect(
        state: &WasmState,
        command: &OnDisconnectCommand,
    ) -> DatabaseResult<String> {
        let body = json!({
            "p": path_to_string(&command.path),
            "d": command.payload.clone(),
        });

        serialize_request(state, command.action.code(), body)
    }

    fn next_request_id(state: &WasmState) -> u32 {
        state.next_request_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    async fn send_initial_tokens(state: Arc<WasmState>, app: FirebaseApp) -> DatabaseResult<()> {
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
        state: &Arc<WasmState>,
        action: &str,
        body: JsonValue,
    ) -> DatabaseResult<()> {
        let message = serialize_request(state.as_ref(), action, body)?;
        let guard = state.socket.lock().await;
        let Some(socket) = guard.as_ref() else {
            return Err(internal_error("websocket sink unavailable"));
        };
        if socket.ready_state() != WebSocket::OPEN {
            return Ok(());
        }
        socket
            .send_with_str(&message)
            .map_err(|err| internal_error(format!("failed to send realtime request: {err:?}")))
    }

    fn serialize_request(
        state: &WasmState,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_lock::Mutex as AsyncMutex;

    #[derive(Clone, Default)]
    struct MockTransport {
        events: Arc<AsyncMutex<Vec<String>>>,
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl RealtimeTransport for MockTransport {
        async fn connect(&self) -> DatabaseResult<()> {
            self.events.lock().await.push("connect".to_string());
            Ok(())
        }

        async fn disconnect(&self) -> DatabaseResult<()> {
            self.events.lock().await.push("disconnect".to_string());
            Ok(())
        }

        async fn listen(&self, _spec: &ListenSpec) -> DatabaseResult<()> {
            Ok(())
        }

        async fn unlisten(&self, _spec: &ListenSpec) -> DatabaseResult<()> {
            Ok(())
        }

        async fn on_disconnect(&self, request: OnDisconnectRequest) -> DatabaseResult<()> {
            let (action, path, payload) = request.into_inner();
            self.events.lock().await.push(format!(
                "on_disconnect:{}:{}:{}",
                action.code(),
                path_to_string(&path),
                payload
            ));
            Ok(())
        }
    }

    #[tokio::test]
    async fn repo_forwards_on_disconnect_command() {
        let transport = Arc::new(MockTransport::default());
        let repo = Repo::new_for_test(transport.clone());

        repo.on_disconnect_put(vec!["messages".into()], JsonValue::Null)
            .await
            .unwrap();

        let events = transport.events.lock().await.clone();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], "connect");
        assert!(events[1].starts_with("on_disconnect:o:/messages"));
    }
}
