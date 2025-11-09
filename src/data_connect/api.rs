#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use async_lock::OnceCell;
use serde_json::Value;

use crate::app;
use crate::app::FirebaseApp;
use crate::app_check::FirebaseAppCheckInternal;
use crate::auth::Auth;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType, Provider};
use crate::data_connect::config::{
    parse_transport_options, ConnectorConfig, DataConnectOptions, TransportOptions,
};
use crate::data_connect::constants::DATA_CONNECT_COMPONENT_NAME;
use crate::data_connect::error::{
    internal_error, DataConnectError, DataConnectErrorCode, DataConnectResult,
};
use crate::data_connect::mutation::MutationManager;
use crate::data_connect::query::{
    cache_from_serialized, QueryManager, QuerySubscriptionHandle, QuerySubscriptionHandlers,
};
use crate::data_connect::reference::{
    MutationRef, OperationRef, OperationType, QueryRef, QueryResult, SerializedQuerySnapshot,
};
use crate::data_connect::transport::{
    AppCheckHeaders, CallerSdkType, DataConnectTransport, RequestTokenProvider, RestTransport,
};

const EMULATOR_ENV: &str = "FIREBASE_DATA_CONNECT_EMULATOR_HOST";

static DATA_CONNECT_CACHE: LazyLock<Mutex<HashMap<(String, String), Arc<DataConnectService>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
thread_local! {
    static QUERY_MANAGER_CACHE: RefCell<HashMap<usize, QueryManager>> = RefCell::new(HashMap::new());
}

#[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
static QUERY_MANAGER_CACHE: LazyLock<Mutex<HashMap<usize, QueryManager>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Primary interface for Data Connect operations.
#[derive(Clone)]
pub struct DataConnectService {
    inner: Arc<DataConnectInner>,
}

/// Owns the cached `QueryManager` for a service, allowing callers to explicitly control when
/// observer state is created or discarded.
pub struct DataConnectQueryRuntime {
    service: Arc<DataConnectService>,
    manager: QueryManager,
    released: bool,
}

impl fmt::Debug for DataConnectService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataConnectService")
            .field("app", &self.app().name())
            .field("connector", &self.inner.options.connector.identifier())
            .finish()
    }
}

struct DataConnectInner {
    app: FirebaseApp,
    options: DataConnectOptions,
    auth_provider: Provider,
    app_check_provider: Provider,
    transport: OnceCell<Arc<dyn DataConnectTransport>>,
    mutation_manager: OnceCell<MutationManager>,
    transport_override: Mutex<Option<TransportOptions>>,
    generated_sdk: AtomicBool,
    caller_sdk_type: Mutex<CallerSdkType>,
}

impl DataConnectService {
    fn new(
        app: FirebaseApp,
        options: DataConnectOptions,
        auth_provider: Provider,
        app_check_provider: Provider,
        env_override: Option<TransportOptions>,
    ) -> Self {
        Self {
            inner: Arc::new(DataConnectInner {
                app,
                options,
                auth_provider,
                app_check_provider,
                transport: OnceCell::new(),
                mutation_manager: OnceCell::new(),
                transport_override: Mutex::new(env_override),
                generated_sdk: AtomicBool::new(false),
                caller_sdk_type: Mutex::new(CallerSdkType::Base),
            }),
        }
    }

    /// Returns the Firebase app associated with this service.
    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    /// Returns the fully qualified connector options.
    pub fn options(&self) -> DataConnectOptions {
        self.inner.options.clone()
    }

    /// Marks this instance as being used by a generated SDK, updating telemetry headers.
    pub fn set_generated_sdk_mode(&self, enabled: bool) {
        self.inner.generated_sdk.store(enabled, Ordering::SeqCst);
        if let Some(transport) = self.inner.transport.get() {
            transport.set_generated_sdk(enabled);
        }
    }

    /// Updates the caller SDK type for telemetry purposes.
    pub fn set_caller_sdk_type(&self, caller: CallerSdkType) {
        *self.inner.caller_sdk_type.lock().unwrap() = caller.clone();
        if let Some(transport) = self.inner.transport.get() {
            transport.set_caller_sdk_type(caller);
        }
    }

    /// Builds a [`DataConnectQueryRuntime`] that keeps the query observer state alive for as long
    /// as the handle is held. Dropping the runtime (or calling [`DataConnectQueryRuntime::close`])
    /// releases the cached manager so a future runtime can start fresh.
    pub async fn query_runtime(&self) -> DataConnectResult<DataConnectQueryRuntime> {
        let service = Arc::new(self.clone());
        DataConnectQueryRuntime::new(service).await
    }

    /// Routes subsequent requests to the specified emulator endpoint.
    pub fn connect_emulator(
        &self,
        host: &str,
        port: Option<u16>,
        ssl_enabled: bool,
    ) -> DataConnectResult<()> {
        let override_options = TransportOptions::new(host, port, ssl_enabled);
        {
            let mut guard = self.inner.transport_override.lock().unwrap();
            if let Some(existing) = guard.as_ref() {
                if existing != &override_options {
                    return Err(DataConnectError::new(
                        DataConnectErrorCode::AlreadyInitialized,
                        "Data Connect instance already initialized",
                    ));
                }
                return Ok(());
            }
            *guard = Some(override_options.clone());
        }

        if let Some(transport) = self.inner.transport.get() {
            transport.use_emulator(override_options);
        }
        Ok(())
    }

    async fn transport(&self) -> DataConnectResult<Arc<dyn DataConnectTransport>> {
        self.inner
            .transport
            .get_or_try_init(|| async {
                let token_provider = Arc::new(TokenBroker::new(
                    self.inner.auth_provider.clone(),
                    self.inner.app_check_provider.clone(),
                ));
                let firebase_options = self.inner.app.options();
                let transport = RestTransport::new(
                    self.inner.options.clone(),
                    firebase_options.api_key,
                    firebase_options.app_id,
                    token_provider,
                )?;
                if let Some(override_options) =
                    self.inner.transport_override.lock().unwrap().clone()
                {
                    transport.use_emulator(override_options);
                }
                transport.set_generated_sdk(self.inner.generated_sdk.load(Ordering::SeqCst));
                transport.set_caller_sdk_type(self.inner.caller_sdk_type.lock().unwrap().clone());
                Ok(Arc::new(transport) as Arc<dyn DataConnectTransport>)
            })
            .await
            .cloned()
    }

    async fn mutation_manager(&self) -> DataConnectResult<MutationManager> {
        let transport = self.transport().await?;
        self.inner
            .mutation_manager
            .get_or_try_init(|| async { Ok(MutationManager::new(transport.clone())) })
            .await
            .cloned()
    }
}

impl DataConnectQueryRuntime {
    async fn new(service: Arc<DataConnectService>) -> DataConnectResult<Self> {
        let manager = query_manager_for_service(&service).await?;
        Ok(Self {
            service,
            manager,
            released: false,
        })
    }

    /// Returns the service associated with this runtime.
    pub fn service(&self) -> &Arc<DataConnectService> {
        &self.service
    }

    /// Executes the provided query reference using this runtime's cached manager.
    pub async fn execute_query(&self, query_ref: &QueryRef) -> DataConnectResult<QueryResult> {
        self.manager.execute_query(query_ref.clone()).await
    }

    /// Subscribes to a query using this runtime's cached manager.
    pub async fn subscribe(
        &self,
        query_ref: QueryRef,
        handlers: QuerySubscriptionHandlers,
        initial_cache: Option<SerializedQuerySnapshot>,
    ) -> DataConnectResult<QuerySubscriptionHandle> {
        let cache = initial_cache
            .as_ref()
            .and_then(|snapshot| cache_from_serialized(snapshot));
        self.manager.subscribe(query_ref, handlers, cache)
    }

    /// Releases the cached manager immediately. Dropping the runtime has the same effect.
    pub fn close(mut self) {
        self.release();
    }

    fn release(&mut self) {
        if !self.released {
            release_query_manager(&self.service);
            self.released = true;
        }
    }
}

impl Drop for DataConnectQueryRuntime {
    fn drop(&mut self) {
        self.release();
    }
}

fn service_cache_key(service: &Arc<DataConnectService>) -> usize {
    Arc::as_ptr(&service.inner) as usize
}

async fn query_manager_for_service(
    service: &Arc<DataConnectService>,
) -> DataConnectResult<QueryManager> {
    let key = service_cache_key(service);

    #[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
    {
        if let Some(manager) = QUERY_MANAGER_CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
            return Ok(manager);
        }

        let transport = service.transport().await?;
        let manager = QueryManager::new(transport);
        QUERY_MANAGER_CACHE.with(|cache| {
            cache.borrow_mut().insert(key, manager.clone());
        });
        Ok(manager)
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
    {
        if let Some(manager) = QUERY_MANAGER_CACHE.lock().unwrap().get(&key).cloned() {
            return Ok(manager);
        }

        let transport = service.transport().await?;
        let manager = QueryManager::new(transport);
        QUERY_MANAGER_CACHE
            .lock()
            .unwrap()
            .insert(key, manager.clone());
        Ok(manager)
    }
}

fn release_query_manager(service: &Arc<DataConnectService>) {
    let key = service_cache_key(service);

    #[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
    QUERY_MANAGER_CACHE.with(|cache| {
        cache.borrow_mut().remove(&key);
    });

    #[cfg(not(all(target_arch = "wasm32", feature = "wasm-web")))]
    {
        QUERY_MANAGER_CACHE.lock().unwrap().remove(&key);
    }
}

/// Constructs a query reference for the specified operation name and variables.
pub fn query_ref(
    service: Arc<DataConnectService>,
    operation_name: impl Into<String>,
    variables: Value,
) -> QueryRef {
    QueryRef(OperationRef {
        service,
        name: Arc::from(operation_name.into()),
        variables,
        op_type: OperationType::Query,
    })
}

/// Constructs a mutation reference for the specified operation name and variables.
pub fn mutation_ref(
    service: Arc<DataConnectService>,
    operation_name: impl Into<String>,
    variables: Value,
) -> MutationRef {
    MutationRef(OperationRef {
        service,
        name: Arc::from(operation_name.into()),
        variables,
        op_type: OperationType::Mutation,
    })
}

/// Executes the provided query reference.
pub async fn execute_query(query_ref: &QueryRef) -> DataConnectResult<QueryResult> {
    query_manager_for_service(query_ref.service())
        .await?
        .execute_query(query_ref.clone())
        .await
}

/// Executes the provided mutation reference.
pub async fn execute_mutation(
    mutation_ref: &MutationRef,
) -> DataConnectResult<crate::data_connect::reference::MutationResult> {
    mutation_ref
        .service()
        .mutation_manager()
        .await?
        .execute_mutation(mutation_ref.clone())
        .await
}

/// Subscribes to a query reference, optionally hydrating from a serialized snapshot.
pub async fn subscribe(
    query_ref: QueryRef,
    handlers: QuerySubscriptionHandlers,
    initial_cache: Option<SerializedQuerySnapshot>,
) -> DataConnectResult<QuerySubscriptionHandle> {
    let cache = initial_cache
        .as_ref()
        .and_then(|snapshot| cache_from_serialized(snapshot));
    query_manager_for_service(query_ref.service())
        .await?
        .subscribe(query_ref, handlers, cache)
}

/// Converts a serialized snapshot back into a live `QueryRef` using the default app.
pub async fn to_query_ref(snapshot: SerializedQuerySnapshot) -> DataConnectResult<QueryRef> {
    let service =
        get_data_connect_service(None, snapshot.ref_info.connector_config.connector.clone())
            .await?;
    Ok(query_ref(
        service,
        snapshot.ref_info.name,
        snapshot.ref_info.variables,
    ))
}

/// Routes all future requests through the emulator configured by the caller.
pub fn connect_data_connect_emulator(
    service: &DataConnectService,
    host: &str,
    port: Option<u16>,
    ssl_enabled: bool,
) -> DataConnectResult<()> {
    service.connect_emulator(host, port, ssl_enabled)
}

pub fn register_data_connect_component() {
    ensure_registered();
}

fn ensure_registered() {
    static REGISTERED: LazyLock<()> = LazyLock::new(|| {
        let component = Component::new(
            DATA_CONNECT_COMPONENT_NAME,
            Arc::new(data_connect_factory),
            ComponentType::Public,
        )
        .with_instantiation_mode(InstantiationMode::Lazy)
        .with_multiple_instances(true);
        let _ = app::register_component(component);
    });
    LazyLock::force(&REGISTERED);
}

fn data_connect_factory(
    container: &crate::component::ComponentContainer,
    options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: DATA_CONNECT_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let connector_config = if !options.options.is_null() {
        serde_json::from_value::<ConnectorConfig>(options.options.clone()).map_err(|err| {
            ComponentError::InitializationFailed {
                name: DATA_CONNECT_COMPONENT_NAME.to_string(),
                reason: format!("invalid connector config: {err}"),
            }
        })?
    } else if let Some(identifier) = options.instance_identifier.as_deref() {
        serde_json::from_str::<ConnectorConfig>(identifier).map_err(|err| {
            ComponentError::InitializationFailed {
                name: DATA_CONNECT_COMPONENT_NAME.to_string(),
                reason: format!("invalid connector identifier: {err}"),
            }
        })?
    } else {
        return Err(ComponentError::InitializationFailed {
            name: DATA_CONNECT_COMPONENT_NAME.to_string(),
            reason: "connector config required".to_string(),
        });
    };

    let project_id =
        app.options()
            .project_id
            .clone()
            .ok_or_else(|| ComponentError::InitializationFailed {
                name: DATA_CONNECT_COMPONENT_NAME.to_string(),
                reason: "project ID must be configured on Firebase options".to_string(),
            })?;
    let options = DataConnectOptions::new(connector_config.clone(), project_id).map_err(|err| {
        ComponentError::InitializationFailed {
            name: DATA_CONNECT_COMPONENT_NAME.to_string(),
            reason: err.to_string(),
        }
    })?;

    let env_override =
        emulator_override_from_env().map_err(|err| ComponentError::InitializationFailed {
            name: DATA_CONNECT_COMPONENT_NAME.to_string(),
            reason: err.to_string(),
        })?;

    let auth_provider = container.get_provider("auth-internal");
    let app_check_provider = container.get_provider("app-check-internal");
    let service = Arc::new(DataConnectService::new(
        (*app).clone(),
        options,
        auth_provider,
        app_check_provider,
        env_override,
    ));
    Ok(service as DynService)
}

fn emulator_override_from_env() -> DataConnectResult<Option<TransportOptions>> {
    match env::var(EMULATOR_ENV) {
        Ok(value) => parse_transport_options(&value).map(Some),
        Err(_) => Ok(None),
    }
}

/// Retrieves (or initializes) a Data Connect service instance for the supplied connector config.
pub async fn get_data_connect_service(
    app: Option<FirebaseApp>,
    config: ConnectorConfig,
) -> DataConnectResult<Arc<DataConnectService>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::get_app(None)
            .await
            .map_err(|err| internal_error(err.to_string()))?,
    };

    let cache_key = (app.name().to_string(), config.identifier());
    if let Some(service) = DATA_CONNECT_CACHE.lock().unwrap().get(&cache_key).cloned() {
        return Ok(service);
    }

    let provider = app::get_provider(&app, DATA_CONNECT_COMPONENT_NAME);
    let identifier = config.identifier();
    if let Some(service) = provider
        .get_immediate_with_options::<DataConnectService>(Some(&identifier), true)
        .unwrap_or(None)
    {
        DATA_CONNECT_CACHE
            .lock()
            .unwrap()
            .insert(cache_key, service.clone());
        return Ok(service);
    }

    let options_value =
        serde_json::to_value(&config).map_err(|err| internal_error(err.to_string()))?;
    match provider.initialize::<DataConnectService>(options_value, Some(&identifier)) {
        Ok(service) => {
            DATA_CONNECT_CACHE
                .lock()
                .unwrap()
                .insert(cache_key, service.clone());
            Ok(service)
        }
        Err(ComponentError::InstanceUnavailable { .. }) => provider
            .get_immediate_with_options::<DataConnectService>(Some(&identifier), true)
            .unwrap_or(None)
            .ok_or_else(|| internal_error("Data Connect instance unavailable"))
            .map(|service| {
                DATA_CONNECT_CACHE
                    .lock()
                    .unwrap()
                    .insert(cache_key, service.clone());
                service
            }),
        Err(err) => Err(internal_error(err.to_string())),
    }
}

struct TokenBroker {
    auth_provider: Provider,
    app_check_provider: Provider,
}

impl TokenBroker {
    fn new(auth_provider: Provider, app_check_provider: Provider) -> Self {
        Self {
            auth_provider,
            app_check_provider,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl RequestTokenProvider for TokenBroker {
    async fn auth_token(&self) -> DataConnectResult<Option<String>> {
        let auth = match self
            .auth_provider
            .get_immediate_with_options::<Auth>(None, true)
        {
            Ok(Some(auth)) => auth,
            Ok(None) => return Ok(None),
            Err(err) => {
                return Err(internal_error(format!(
                    "failed to resolve auth provider: {err}"
                )))
            }
        };

        auth.get_token(false)
            .await
            .map_err(|err| internal_error(err.to_string()))
    }

    async fn app_check_headers(&self) -> DataConnectResult<Option<AppCheckHeaders>> {
        let app_check = match self
            .app_check_provider
            .get_immediate_with_options::<FirebaseAppCheckInternal>(None, true)
        {
            Ok(Some(app_check)) => app_check,
            Ok(None) => return Ok(None),
            Err(err) => {
                return Err(internal_error(format!(
                    "failed to resolve app check provider: {err}"
                )))
            }
        };

        let token = match app_check.get_token(false).await {
            Ok(result) => result.token,
            Err(err) => {
                if let Some(cached) = err.cached_token() {
                    cached.token.clone()
                } else {
                    return Err(internal_error(format!(
                        "failed to obtain App Check token: {err}"
                    )));
                }
            }
        };

        if token.is_empty() {
            return Ok(None);
        }

        let heartbeat = app_check
            .heartbeat_header()
            .await
            .map_err(|err| internal_error(err.to_string()))?;
        Ok(Some(AppCheckHeaders { token, heartbeat }))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::app::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use httpmock::prelude::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::sync::oneshot;

    fn unique_settings(prefix: &str) -> FirebaseAppSettings {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "{prefix}-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    fn base_options() -> FirebaseOptions {
        FirebaseOptions {
            project_id: Some("demo-project".into()),
            api_key: Some("demo-key".into()),
            ..Default::default()
        }
    }

    fn clear_caches() {
        DATA_CONNECT_CACHE.lock().unwrap().clear();
        QUERY_MANAGER_CACHE.lock().unwrap().clear();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_query_hits_emulator() {
        clear_caches();
        let app = initialize_app(base_options(), Some(unique_settings("dc-query")))
            .await
            .unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path(
                "/v1/projects/demo-project/locations/us-central1/services/catalog/connectors/books:executeQuery",
            );
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "data": {"items": [{"id": "123"}]}
                }));
        });

        let config = ConnectorConfig::new("us-central1", "books", "catalog").unwrap();
        let service = get_data_connect_service(Some(app.clone()), config)
            .await
            .unwrap();
        let host = server.host();
        service
            .connect_emulator(&host, Some(server.port()), false)
            .unwrap();

        let query = query_ref(service, "ListItems", Value::Null);
        let result = execute_query(&query).await.unwrap();
        assert_eq!(result.data["items"][0]["id"], "123");
        mock.assert();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execute_mutation_hits_emulator() {
        clear_caches();
        let app = initialize_app(base_options(), Some(unique_settings("dc-mutation")))
            .await
            .unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path(
                "/v1/projects/demo-project/locations/us-central1/services/catalog/connectors/books:executeMutation",
            );
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "data": {"insertBook": {"id": "321"}}
                }));
        });

        let config = ConnectorConfig::new("us-central1", "books", "catalog").unwrap();
        let service = get_data_connect_service(Some(app.clone()), config)
            .await
            .unwrap();
        let host = server.host();
        service
            .connect_emulator(&host, Some(server.port()), false)
            .unwrap();

        let mutation = mutation_ref(service, "InsertBook", json!({"id": "321"}));
        let result = execute_mutation(&mutation).await.unwrap();
        assert_eq!(result.data["insertBook"]["id"], "321");
        mock.assert();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn subscribe_with_initial_cache_invokes_handler() {
        clear_caches();
        let app = initialize_app(base_options(), Some(unique_settings("dc-subscribe")))
            .await
            .unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path(
                "/v1/projects/demo-project/locations/us-central1/services/catalog/connectors/books:executeQuery",
            );
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "data": {"items": [{"id": "cached"}]}
                }));
        });

        let config = ConnectorConfig::new("us-central1", "books", "catalog").unwrap();
        let service = get_data_connect_service(Some(app.clone()), config)
            .await
            .unwrap();
        let host = server.host();
        service
            .connect_emulator(&host, Some(server.port()), false)
            .unwrap();

        let query = query_ref(service.clone(), "ListItems", Value::Null);
        let snapshot = execute_query(&query).await.unwrap().to_serialized();
        mock.assert();

        let query = query_ref(service, "ListItems", Value::Null);
        let (tx, rx) = oneshot::channel();
        let sender = Arc::new(StdMutex::new(Some(tx)));
        let handlers = QuerySubscriptionHandlers::new(Arc::new(move |result: &QueryResult| {
            if let Some(tx) = sender.lock().unwrap().take() {
                let _ = tx.send(result.data.clone());
            }
        }));

        let handle = subscribe(query, handlers, Some(snapshot)).await.unwrap();
        let data = rx.await.unwrap();
        assert_eq!(data["items"][0]["id"], "cached");
        drop(handle);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn emitter_cannot_change_after_initialization() {
        clear_caches();
        let app = initialize_app(base_options(), Some(unique_settings("dc-emulator")))
            .await
            .unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path(
                "/v1/projects/demo-project/locations/us-central1/services/catalog/connectors/books:executeQuery",
            );
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({ "data": {"ok": true} }));
        });

        let config = ConnectorConfig::new("us-central1", "books", "catalog").unwrap();
        let service = get_data_connect_service(Some(app.clone()), config)
            .await
            .unwrap();
        let host = server.host();
        service
            .connect_emulator(&host, Some(server.port()), false)
            .unwrap();

        let query = query_ref(service.clone(), "ListItems", Value::Null);
        let _ = execute_query(&query).await.unwrap();
        mock.assert();

        let err = service
            .connect_emulator("127.0.0.1", Some(9000), false)
            .unwrap_err();
        assert_eq!(err.code(), DataConnectErrorCode::AlreadyInitialized);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn query_runtime_executes_and_releases() {
        clear_caches();
        let app = initialize_app(base_options(), Some(unique_settings("dc-runtime")))
            .await
            .unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path(
                "/v1/projects/demo-project/locations/us-central1/services/catalog/connectors/books:executeQuery",
            );
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "data": {"items": [{"id": "from-runtime"}]}
                }));
        });

        let config = ConnectorConfig::new("us-central1", "books", "catalog").unwrap();
        let service = get_data_connect_service(Some(app.clone()), config)
            .await
            .unwrap();
        let host = server.host();
        service
            .connect_emulator(&host, Some(server.port()), false)
            .unwrap();

        let runtime = service.query_runtime().await.unwrap();
        let query = query_ref(runtime.service().clone(), "ListItems", Value::Null);
        let result = runtime.execute_query(&query).await.unwrap();
        assert_eq!(result.data["items"][0]["id"], "from-runtime");
        mock.assert();

        let key = service_cache_key(runtime.service());
        assert!(QUERY_MANAGER_CACHE.lock().unwrap().contains_key(&key));
        runtime.close();
        assert!(!QUERY_MANAGER_CACHE.lock().unwrap().contains_key(&key));

        // A new runtime recreates the manager seamlessly.
        let runtime2 = service.query_runtime().await.unwrap();
        let key2 = service_cache_key(runtime2.service());
        assert!(QUERY_MANAGER_CACHE.lock().unwrap().contains_key(&key2));
        drop(runtime2);
    }
}
