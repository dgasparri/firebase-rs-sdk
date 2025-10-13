use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::remote_config::constants::REMOTE_CONFIG_COMPONENT_NAME;
use crate::remote_config::error::{internal_error, RemoteConfigResult};

#[derive(Clone, Debug)]
pub struct RemoteConfig {
    inner: Arc<RemoteConfigInner>,
}

#[derive(Debug)]
struct RemoteConfigInner {
    app: FirebaseApp,
    defaults: Mutex<HashMap<String, String>>,
    values: Mutex<HashMap<String, String>>,
    activated: Mutex<bool>,
}
static REMOTE_CONFIG_CACHE: LazyLock<Mutex<HashMap<String, Arc<RemoteConfig>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl RemoteConfig {
    fn new(app: FirebaseApp) -> Self {
        Self {
            inner: Arc::new(RemoteConfigInner {
                app,
                defaults: Mutex::new(HashMap::new()),
                values: Mutex::new(HashMap::new()),
                activated: Mutex::new(false),
            }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn set_defaults(&self, defaults: HashMap<String, String>) {
        *self.inner.defaults.lock().unwrap() = defaults;
    }

    pub fn fetch(&self) -> RemoteConfigResult<()> {
        // Minimal stub: mark values as fetched but keep defaults.
        Ok(())
    }

    pub fn activate(&self) -> RemoteConfigResult<bool> {
        let mut activated = self.inner.activated.lock().unwrap();
        let changed = !*activated;
        if changed {
            *self.inner.values.lock().unwrap() = self.inner.defaults.lock().unwrap().clone();
        }
        *activated = true;
        Ok(changed)
    }

    pub fn get_string(&self, key: &str) -> String {
        if let Some(value) = self.inner.values.lock().unwrap().get(key) {
            return value.clone();
        }
        self.inner
            .defaults
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .unwrap_or_default()
    }
}

static REMOTE_CONFIG_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        REMOTE_CONFIG_COMPONENT_NAME,
        Arc::new(remote_config_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = app::registry::register_component(component);
});

fn remote_config_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: REMOTE_CONFIG_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let rc = RemoteConfig::new((*app).clone());
    Ok(Arc::new(rc) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&REMOTE_CONFIG_COMPONENT);
}

pub fn register_remote_config_component() {
    ensure_registered();
}

pub fn get_remote_config(app: Option<FirebaseApp>) -> RemoteConfigResult<Arc<RemoteConfig>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    if let Some(rc) = REMOTE_CONFIG_CACHE.lock().unwrap().get(app.name()).cloned() {
        return Ok(rc);
    }

    let provider = app::registry::get_provider(&app, REMOTE_CONFIG_COMPONENT_NAME);
    if let Some(rc) = provider.get_immediate::<RemoteConfig>() {
        REMOTE_CONFIG_CACHE
            .lock()
            .unwrap()
            .insert(app.name().to_string(), rc.clone());
        return Ok(rc);
    }

    match provider.initialize::<RemoteConfig>(serde_json::Value::Null, None) {
        Ok(rc) => {
            REMOTE_CONFIG_CACHE
                .lock()
                .unwrap()
                .insert(app.name().to_string(), rc.clone());
            Ok(rc)
        }
        Err(crate::component::types::ComponentError::InstanceUnavailable { .. }) => {
            if let Some(rc) = provider.get_immediate::<RemoteConfig>() {
                REMOTE_CONFIG_CACHE
                    .lock()
                    .unwrap()
                    .insert(app.name().to_string(), rc.clone());
                Ok(rc)
            } else {
                let rc = Arc::new(RemoteConfig::new(app.clone()));
                REMOTE_CONFIG_CACHE
                    .lock()
                    .unwrap()
                    .insert(app.name().to_string(), rc.clone());
                Ok(rc)
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
                "remote-config-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn defaults_activate() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = get_remote_config(Some(app)).unwrap();
        rc.set_defaults(HashMap::from([(
            String::from("welcome"),
            String::from("hello"),
        )]));
        rc.fetch().unwrap();
        assert!(rc.activate().unwrap());
        assert_eq!(rc.get_string("welcome"), "hello");
    }

    #[test]
    fn activate_after_defaults_returns_false() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = get_remote_config(Some(app)).unwrap();
        rc.set_defaults(HashMap::from([(String::from("flag"), String::from("off"))]));
        rc.fetch().unwrap();
        rc.activate().unwrap();
        assert!(!rc.activate().unwrap());
    }
}
