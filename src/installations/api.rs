use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime};

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::installations::constants::INSTALLATIONS_COMPONENT_NAME;
use crate::installations::error::{internal_error, InstallationsResult};

#[derive(Clone, Debug)]
pub struct Installations {
    inner: Arc<InstallationsInner>,
}

#[derive(Debug)]
struct InstallationsInner {
    app: FirebaseApp,
    state: Mutex<HashMap<String, InstallationEntry>>,
}

static INSTALLATIONS_CACHE: LazyLock<Mutex<HashMap<String, Arc<Installations>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug)]
struct InstallationEntry {
    fid: String,
    token: InstallationToken,
}

#[derive(Clone, Debug)]
pub struct InstallationToken {
    pub token: String,
    pub expires_at: SystemTime,
}

impl Installations {
    fn new(app: FirebaseApp) -> Self {
        Self {
            inner: Arc::new(InstallationsInner {
                app,
                state: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn get_id(&self) -> InstallationsResult<String> {
        let mut state = self.inner.state.lock().unwrap();
        let entry = state
            .entry(self.inner.app.name().to_string())
            .or_insert_with(|| {
                let fid = generate_fid();
                InstallationEntry {
                    fid: fid.clone(),
                    token: generate_token(),
                }
            });
        Ok(entry.fid.clone())
    }

    pub fn get_token(&self, force_refresh: bool) -> InstallationsResult<InstallationToken> {
        let mut state = self.inner.state.lock().unwrap();
        let entry = state
            .entry(self.inner.app.name().to_string())
            .or_insert_with(|| {
                let fid = generate_fid();
                InstallationEntry {
                    fid: fid.clone(),
                    token: generate_token(),
                }
            });
        if force_refresh || entry.token.expires_at <= SystemTime::now() {
            entry.token = generate_token();
        }
        Ok(entry.token.clone())
    }
}

fn generate_fid() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(22)
        .collect()
}

fn generate_token() -> InstallationToken {
    let token: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(40)
        .collect();
    InstallationToken {
        token,
        expires_at: SystemTime::now() + Duration::from_secs(60 * 60),
    }
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
    let installations = Installations::new((*app).clone());
    Ok(Arc::new(installations) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&INSTALLATIONS_COMPONENT);
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
                let fallback = Arc::new(Installations::new(app.clone()));
                INSTALLATIONS_CACHE
                    .lock()
                    .unwrap()
                    .insert(app.name().to_string(), fallback.clone());
                Ok(fallback)
            }
        }
        Err(err) => Err(internal_error(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::initialize_app;
    use crate::app::{FirebaseAppSettings, FirebaseOptions};

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

    #[test]
    fn get_id_generates_consistent_fid() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let installations = get_installations(Some(app)).unwrap();
        let fid1 = installations.get_id().unwrap();
        let fid2 = installations.get_id().unwrap();
        assert_eq!(fid1, fid2);
    }

    #[test]
    fn token_refreshes() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let installations = get_installations(Some(app)).unwrap();
        let token1 = installations.get_token(false).unwrap();
        let token2 = installations.get_token(true).unwrap();
        assert_ne!(token1.token, token2.token);
    }
}
