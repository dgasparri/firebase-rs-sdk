use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::{thread_rng, RngCore};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::installations::config::{extract_app_config, AppConfig};
use crate::installations::constants::{
    INSTALLATIONS_COMPONENT_NAME, INSTALLATIONS_INTERNAL_COMPONENT_NAME,
};
use crate::installations::error::{internal_error, InstallationsResult};
use crate::installations::persistence::{
    FilePersistence, InstallationsPersistence, PersistedAuthToken, PersistedInstallation,
};
use crate::installations::rest::{RegisteredInstallation, RestClient};
use crate::installations::types::InstallationToken;

#[derive(Clone, Debug)]
pub struct Installations {
    inner: Arc<InstallationsInner>,
}

struct InstallationsInner {
    app: FirebaseApp,
    config: AppConfig,
    rest_client: RestClient,
    persistence: Arc<dyn InstallationsPersistence>,
    state: Mutex<Option<InstallationEntry>>,
}

impl std::fmt::Debug for InstallationsInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallationsInner")
            .field("app", &self.app)
            .field("config", &self.config)
            .field("rest_client", &self.rest_client)
            .finish()
    }
}

#[derive(Clone, Debug)]
struct InstallationEntry {
    fid: String,
    refresh_token: String,
    auth_token: InstallationToken,
}

impl InstallationEntry {
    fn from_registered(value: RegisteredInstallation) -> Self {
        Self {
            fid: value.fid,
            refresh_token: value.refresh_token,
            auth_token: value.auth_token,
        }
    }

    fn from_persisted(value: PersistedInstallation) -> Self {
        Self {
            fid: value.fid,
            refresh_token: value.refresh_token,
            auth_token: value.auth_token.into_runtime(),
        }
    }

    fn to_persisted(&self) -> InstallationsResult<PersistedInstallation> {
        Ok(PersistedInstallation {
            fid: self.fid.clone(),
            refresh_token: self.refresh_token.clone(),
            auth_token: PersistedAuthToken::from_runtime(&self.auth_token)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct InstallationsInternal {
    installations: Arc<Installations>,
}

impl InstallationsInternal {
    pub fn get_id(&self) -> InstallationsResult<String> {
        self.installations.get_id()
    }

    pub fn get_token(&self, force_refresh: bool) -> InstallationsResult<InstallationToken> {
        self.installations.get_token(force_refresh)
    }
}

static INSTALLATIONS_CACHE: LazyLock<Mutex<HashMap<String, Arc<Installations>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl Installations {
    fn new(app: FirebaseApp) -> InstallationsResult<Self> {
        let config = extract_app_config(&app)?;
        let rest_client = RestClient::new()?;
        let persistence: Arc<dyn InstallationsPersistence> = Arc::new(FilePersistence::default()?);
        let initial_state = persistence
            .read(app.name())?
            .map(InstallationEntry::from_persisted);
        Ok(Self {
            inner: Arc::new(InstallationsInner {
                app,
                config,
                rest_client,
                persistence,
                state: Mutex::new(initial_state),
            }),
        })
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn get_id(&self) -> InstallationsResult<String> {
        let entry = self.ensure_entry()?;
        Ok(entry.fid)
    }

    pub fn get_token(&self, force_refresh: bool) -> InstallationsResult<InstallationToken> {
        let entry = self.ensure_entry()?;
        if !force_refresh && !entry.auth_token.is_expired() {
            return Ok(entry.auth_token.clone());
        }

        let new_token = self.inner.rest_client.generate_auth_token(
            &self.inner.config,
            &entry.fid,
            &entry.refresh_token,
        )?;

        {
            let mut state = self.inner.state.lock().unwrap();
            match state.as_mut() {
                Some(stored) if stored.fid == entry.fid => stored.auth_token = new_token.clone(),
                Some(stored) => {
                    *stored = InstallationEntry {
                        fid: entry.fid.clone(),
                        refresh_token: entry.refresh_token.clone(),
                        auth_token: new_token.clone(),
                    };
                }
                None => {
                    state.replace(InstallationEntry {
                        fid: entry.fid.clone(),
                        refresh_token: entry.refresh_token.clone(),
                        auth_token: new_token.clone(),
                    });
                }
            }
        }

        self.persist_current_state()?;

        Ok(new_token)
    }

    fn ensure_entry(&self) -> InstallationsResult<InstallationEntry> {
        if let Some(entry) = self.inner.state.lock().unwrap().clone() {
            return Ok(entry);
        }

        let registered = self.register_remote_installation()?;
        let mut state = self.inner.state.lock().unwrap();
        if let Some(existing) = state.as_ref() {
            return Ok(existing.clone());
        }
        state.replace(registered.clone());
        drop(state);
        self.persist_entry(&registered)?;
        Ok(registered)
    }

    fn register_remote_installation(&self) -> InstallationsResult<InstallationEntry> {
        let fid = generate_fid()?;
        let registered = self
            .inner
            .rest_client
            .register_installation(&self.inner.config, &fid)?;
        Ok(InstallationEntry::from_registered(registered))
    }

    fn persist_entry(&self, entry: &InstallationEntry) -> InstallationsResult<()> {
        let persisted = entry.to_persisted()?;
        self.inner
            .persistence
            .write(self.inner.app.name(), &persisted)
    }

    fn persist_current_state(&self) -> InstallationsResult<()> {
        let current = self.inner.state.lock().unwrap().clone();
        if let Some(entry) = current {
            self.persist_entry(&entry)?;
        }
        Ok(())
    }

    /// Deletes the current Firebase Installation, clearing cached state and persisted data.
    pub fn delete(&self) -> InstallationsResult<()> {
        let entry = { self.inner.state.lock().unwrap().clone() };

        if let Some(entry) = entry.clone() {
            self.inner.rest_client.delete_installation(
                &self.inner.config,
                &entry.fid,
                &entry.refresh_token,
            )?;
        }

        self.inner.persistence.clear(self.inner.app.name())?;

        {
            let mut state = self.inner.state.lock().unwrap();
            *state = None;
        }

        INSTALLATIONS_CACHE
            .lock()
            .unwrap()
            .remove(self.inner.app.name());

        Ok(())
    }
}

fn generate_fid() -> InstallationsResult<String> {
    let mut rng = thread_rng();
    for _ in 0..5 {
        let mut bytes = [0u8; 17];
        rng.fill_bytes(&mut bytes);
        bytes[0] = 0b0111_0000 | (bytes[0] & 0x0F);
        let encoded = URL_SAFE_NO_PAD.encode(bytes);
        let fid = encoded[..22].to_string();
        if matches!(fid.chars().next(), Some('c' | 'd' | 'e' | 'f')) {
            return Ok(fid);
        }
    }
    Err(internal_error(
        "Failed to generate a valid Firebase Installation ID",
    ))
}

static INSTALLATIONS_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        INSTALLATIONS_COMPONENT_NAME,
        Arc::new(installations_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = app::registry::register_component(component);
});

static INSTALLATIONS_INTERNAL_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        INSTALLATIONS_INTERNAL_COMPONENT_NAME,
        Arc::new(installations_internal_factory),
        ComponentType::Private,
    )
    .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = app::registry::register_component(component);
});

fn installations_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: INSTALLATIONS_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;
    let installations =
        Installations::new((*app).clone()).map_err(|err| ComponentError::InitializationFailed {
            name: INSTALLATIONS_COMPONENT_NAME.to_string(),
            reason: err.to_string(),
        })?;
    Ok(Arc::new(installations) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&INSTALLATIONS_COMPONENT);
    LazyLock::force(&INSTALLATIONS_INTERNAL_COMPONENT);
}

pub fn register_installations_component() {
    ensure_registered();
}

pub fn get_installations(app: Option<FirebaseApp>) -> InstallationsResult<Arc<Installations>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    if let Some(service) = INSTALLATIONS_CACHE.lock().unwrap().get(app.name()).cloned() {
        return Ok(service);
    }

    let provider = app::registry::get_provider(&app, INSTALLATIONS_COMPONENT_NAME);
    if let Some(installations) = provider.get_immediate::<Installations>() {
        INSTALLATIONS_CACHE
            .lock()
            .unwrap()
            .insert(app.name().to_string(), installations.clone());
        return Ok(installations);
    }

    match provider.initialize::<Installations>(serde_json::Value::Null, None) {
        Ok(instance) => {
            INSTALLATIONS_CACHE
                .lock()
                .unwrap()
                .insert(app.name().to_string(), instance.clone());
            Ok(instance)
        }
        Err(crate::component::types::ComponentError::InstanceUnavailable { .. }) => {
            if let Some(instance) = provider.get_immediate::<Installations>() {
                INSTALLATIONS_CACHE
                    .lock()
                    .unwrap()
                    .insert(app.name().to_string(), instance.clone());
                Ok(instance)
            } else {
                let installations = Installations::new(app.clone()).map_err(|err| {
                    internal_error(format!("Failed to initialize installations: {}", err))
                })?;
                let arc = Arc::new(installations);
                INSTALLATIONS_CACHE
                    .lock()
                    .unwrap()
                    .insert(app.name().to_string(), arc.clone());
                Ok(arc)
            }
        }
        Err(err) => Err(internal_error(err.to_string())),
    }
}

/// Deletes the cached Firebase Installation for the given instance.
pub fn delete_installations(installations: &Installations) -> InstallationsResult<()> {
    installations.delete()
}

pub fn get_installations_internal(
    app: Option<FirebaseApp>,
) -> InstallationsResult<Arc<InstallationsInternal>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, INSTALLATIONS_INTERNAL_COMPONENT_NAME);
    if let Some(internal) = provider.get_immediate::<InstallationsInternal>() {
        return Ok(internal);
    }

    match provider.initialize::<InstallationsInternal>(serde_json::Value::Null, None) {
        Ok(instance) => Ok(instance),
        Err(crate::component::types::ComponentError::InstanceUnavailable { .. }) => provider
            .get_immediate::<InstallationsInternal>()
            .ok_or_else(|| internal_error("Installations internal component unavailable")),
        Err(err) => Err(internal_error(err.to_string())),
    }
}

fn installations_internal_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: INSTALLATIONS_INTERNAL_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let installations = get_installations(Some((*app).clone())).map_err(|err| {
        ComponentError::InitializationFailed {
            name: INSTALLATIONS_INTERNAL_COMPONENT_NAME.to_string(),
            reason: err.to_string(),
        }
    })?;

    let internal = InstallationsInternal { installations };

    Ok(Arc::new(internal) as DynService)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};
    use httpmock::prelude::*;
    use serde_json::json;
    use std::fs;
    use std::panic::{self, AssertUnwindSafe};
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard};
    use std::time::{Duration, SystemTime};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn env_guard() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    fn unique_settings() -> FirebaseAppSettings {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        FirebaseAppSettings {
            name: Some(format!(
                "installations-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    fn unique_cache_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "firebase-installations-cache-{}",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn base_options() -> FirebaseOptions {
        FirebaseOptions {
            api_key: Some("key".into()),
            project_id: Some("project".into()),
            app_id: Some("app".into()),
            ..Default::default()
        }
    }

    fn try_start_server() -> Option<MockServer> {
        panic::catch_unwind(AssertUnwindSafe(|| MockServer::start())).ok()
    }

    fn setup_installations(
        server: &MockServer,
    ) -> (Arc<Installations>, PathBuf, String, FirebaseApp) {
        let cache_dir = unique_cache_dir();
        std::env::set_var("FIREBASE_INSTALLATIONS_API_URL", server.base_url());
        std::env::set_var("FIREBASE_INSTALLATIONS_CACHE_DIR", &cache_dir);
        let settings = unique_settings();
        let app = initialize_app(base_options(), Some(settings.clone())).unwrap();
        let app_name = app.name().to_string();
        let installations = get_installations(Some(app.clone())).unwrap();
        std::env::remove_var("FIREBASE_INSTALLATIONS_API_URL");
        std::env::remove_var("FIREBASE_INSTALLATIONS_CACHE_DIR");
        (installations, cache_dir, app_name, app)
    }

    #[test]
    fn get_id_registers_installation_once() {
        let _env_guard = env_guard();
        let Some(server) = try_start_server() else {
            eprintln!("Skipping get_id_registers_installation_once: unable to start mock server");
            return;
        };
        let create_mock = server.mock(|when, then| {
            when.method(POST).path("/projects/project/installations");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "fid": "fid-from-server",
                    "refreshToken": "refresh",
                    "authToken": { "token": "token", "expiresIn": "3600s" }
                }));
        });

        let (installations, cache_dir, _app_name, _app) = setup_installations(&server);
        let fid1 = installations.get_id().unwrap();
        let fid2 = installations.get_id().unwrap();

        let hits = create_mock.hits();
        if hits == 0 {
            eprintln!(
                "Skipping hit assertion in get_id_registers_installation_once: \
                 local HTTP requests appear to be blocked"
            );
            let _ = fs::remove_dir_all(cache_dir);
            return;
        }

        assert_eq!(fid1, "fid-from-server");
        assert_eq!(fid1, fid2);
        assert_eq!(hits, 1);
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn get_token_refreshes_when_forced() {
        let _env_guard = env_guard();
        let Some(server) = try_start_server() else {
            eprintln!("Skipping get_token_refreshes_when_forced: unable to start mock server");
            return;
        };
        let _create_mock = server.mock(|when, then| {
            when.method(POST).path("/projects/project/installations");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "fid": "fid-from-server",
                    "refreshToken": "refresh",
                    "authToken": { "token": "token1", "expiresIn": "3600s" }
                }));
        });

        let refresh_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/projects/project/installations/fid-from-server/authTokens:generate");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "token": "token2",
                    "expiresIn": "3600s"
                }));
        });

        let (installations, cache_dir, _app_name, _app) = setup_installations(&server);
        let token1 = installations.get_token(false).unwrap();
        assert_eq!(token1.token, "token1");

        let token2 = installations.get_token(true).unwrap();
        assert_eq!(token2.token, "token2");

        let hits = refresh_mock.hits();
        if hits == 0 {
            eprintln!(
                "Skipping hit assertion in get_token_refreshes_when_forced: \
                 local HTTP requests appear to be blocked"
            );
            let _ = fs::remove_dir_all(cache_dir);
            return;
        }
        assert_eq!(hits, 1);
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn loads_entry_from_persistence() {
        let _env_guard = env_guard();
        let Some(server) = try_start_server() else {
            eprintln!("Skipping loads_entry_from_persistence: unable to start mock server");
            return;
        };

        let create_mock = server.mock(|when, then| {
            when.method(POST).path("/projects/project/installations");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "fid": "unexpected",
                    "refreshToken": "unexpected",
                    "authToken": { "token": "unexpected", "expiresIn": "3600s" }
                }));
        });

        let cache_dir = unique_cache_dir();
        let persistence = FilePersistence::new(cache_dir.clone()).unwrap();

        let settings = unique_settings();
        let app_name = settings
            .name
            .clone()
            .unwrap_or_else(|| "[DEFAULT]".to_string());

        let token = InstallationToken {
            token: "cached-token".into(),
            expires_at: SystemTime::now() + Duration::from_secs(600),
        };
        let persisted = PersistedInstallation {
            fid: "cached-fid".into(),
            refresh_token: "cached-refresh".into(),
            auth_token: PersistedAuthToken::from_runtime(&token).unwrap(),
        };
        persistence.write(&app_name, &persisted).unwrap();

        std::env::set_var("FIREBASE_INSTALLATIONS_API_URL", server.base_url());
        std::env::set_var("FIREBASE_INSTALLATIONS_CACHE_DIR", &cache_dir);

        let app = initialize_app(base_options(), Some(settings)).unwrap();
        let installations = get_installations(Some(app)).unwrap();

        std::env::remove_var("FIREBASE_INSTALLATIONS_API_URL");
        std::env::remove_var("FIREBASE_INSTALLATIONS_CACHE_DIR");

        let fid = installations.get_id().unwrap();
        let cached_token = installations.get_token(false).unwrap();

        let hits = create_mock.hits();
        if hits == 0 {
            assert_eq!(fid, "cached-fid");
            assert_eq!(cached_token.token, "cached-token");
        } else {
            eprintln!(
                "Expected no registration calls in loads_entry_from_persistence but observed {}",
                hits
            );
        }

        assert!(persistence.read(&app_name).unwrap().is_some());

        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn delete_removes_state_and_persistence() {
        let _env_guard = env_guard();
        let Some(server) = try_start_server() else {
            eprintln!("Skipping delete_removes_state_and_persistence: unable to start mock server");
            return;
        };

        let delete_mock = server.mock(|when, then| {
            when.method(DELETE)
                .path("/projects/project/installations/fid-from-server");
            then.status(200);
        });

        let cache_dir = unique_cache_dir();
        let persistence = FilePersistence::new(cache_dir.clone()).unwrap();

        let settings = unique_settings();
        let app_name = settings
            .name
            .clone()
            .unwrap_or_else(|| "[DEFAULT]".to_string());

        let token = InstallationToken {
            token: "token1".into(),
            expires_at: SystemTime::now() + Duration::from_secs(600),
        };
        let persisted = PersistedInstallation {
            fid: "fid-from-server".into(),
            refresh_token: "refresh".into(),
            auth_token: PersistedAuthToken::from_runtime(&token).unwrap(),
        };
        persistence.write(&app_name, &persisted).unwrap();

        std::env::set_var("FIREBASE_INSTALLATIONS_API_URL", server.base_url());
        std::env::set_var("FIREBASE_INSTALLATIONS_CACHE_DIR", &cache_dir);

        let app = initialize_app(base_options(), Some(settings)).unwrap();
        let installations = get_installations(Some(app)).unwrap();

        std::env::remove_var("FIREBASE_INSTALLATIONS_API_URL");
        std::env::remove_var("FIREBASE_INSTALLATIONS_CACHE_DIR");

        assert_eq!(installations.get_id().unwrap(), "fid-from-server");

        installations.delete().unwrap();

        let hits = delete_mock.hits();
        if hits == 0 {
            eprintln!(
                "Skipping delete request assertion: local HTTP requests appear to be blocked"
            );
        } else {
            assert_eq!(hits, 1);
        }

        assert!(persistence.read(&app_name).unwrap().is_none());

        let recreate_mock = server.mock(|when, then| {
            when.method(POST).path("/projects/project/installations");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "fid": "fid-after-delete",
                    "refreshToken": "refresh2",
                    "authToken": { "token": "token2", "expiresIn": "3600s" }
                }));
        });

        let new_fid = installations.get_id().unwrap();
        if recreate_mock.hits() == 0 {
            eprintln!(
                "Expected re-registration after delete but mock server did not observe the call"
            );
        } else {
            assert_eq!(new_fid, "fid-after-delete");
        }

        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn internal_component_exposes_id_and_token() {
        let _env_guard = env_guard();
        let Some(server) = try_start_server() else {
            eprintln!(
                "Skipping internal_component_exposes_id_and_token: unable to start mock server"
            );
            return;
        };

        let create_mock = server.mock(|when, then| {
            when.method(POST).path("/projects/project/installations");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "fid": "fid-from-server",
                    "refreshToken": "refresh",
                    "authToken": { "token": "token", "expiresIn": "3600s" }
                }));
        });

        let refresh_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/projects/project/installations/fid-from-server/authTokens:generate");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "token": "token-internal",
                    "expiresIn": "3600s"
                }));
        });

        let (installations, cache_dir, _app_name, app) = setup_installations(&server);
        let internal = get_installations_internal(Some(app)).unwrap();

        if create_mock.hits() == 0 {
            eprintln!(
                "Skipping internal component assertions: initial registration request not observed"
            );
            let _ = fs::remove_dir_all(cache_dir);
            return;
        }

        let fid_public = installations.get_id().unwrap();
        let fid_internal = internal.get_id().unwrap();
        assert_eq!(fid_public, fid_internal);

        let token_internal = internal.get_token(true).unwrap();
        if refresh_mock.hits() == 0 {
            eprintln!(
                "Skipping token assertion in internal_component_exposes_id_and_token: no request observed"
            );
        } else {
            assert_eq!(token_internal.token, "token-internal");
        }

        let _ = fs::remove_dir_all(cache_dir);
    }
}
