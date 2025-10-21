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
use crate::remote_config::settings::{RemoteConfigSettings, RemoteConfigSettingsUpdate};
use crate::remote_config::value::{RemoteConfigValue, RemoteConfigValueSource};

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
    settings: Mutex<RemoteConfigSettings>,
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
                settings: Mutex::new(RemoteConfigSettings::default()),
            }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn set_defaults(&self, defaults: HashMap<String, String>) {
        *self.inner.defaults.lock().unwrap() = defaults;
    }

    /// Returns a copy of the current Remote Config settings.
    ///
    /// Mirrors the JS `remoteConfig.settings` property.
    pub fn settings(&self) -> RemoteConfigSettings {
        self.inner.settings.lock().unwrap().clone()
    }

    /// Applies validated settings to the Remote Config instance.
    ///
    /// Equivalent to the JS `setConfigSettings` helper. Values are merged with the existing
    /// configuration and validated before being applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use firebase_rs_sdk_unofficial::remote_config::settings::RemoteConfigSettingsUpdate;
    /// # use firebase_rs_sdk_unofficial::remote_config::get_remote_config;
    /// # use firebase_rs_sdk_unofficial::app::api::initialize_app;
    /// # use firebase_rs_sdk_unofficial::app::{FirebaseOptions, FirebaseAppSettings};
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let app = initialize_app(FirebaseOptions::default(), Some(FirebaseAppSettings::default()))?;
    /// let rc = get_remote_config(Some(app))?;
    /// rc.set_config_settings(RemoteConfigSettingsUpdate {
    ///     fetch_timeout_millis: Some(90_000),
    ///     minimum_fetch_interval_millis: Some(3_600_000),
    /// })?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_config_settings(
        &self,
        update: RemoteConfigSettingsUpdate,
    ) -> RemoteConfigResult<()> {
        if update.is_empty() {
            return Ok(());
        }

        let mut settings = self.inner.settings.lock().unwrap();

        if let Some(fetch_timeout) = update.fetch_timeout_millis {
            settings.set_fetch_timeout_millis(fetch_timeout)?;
        }

        if let Some(min_interval) = update.minimum_fetch_interval_millis {
            settings.set_minimum_fetch_interval_millis(min_interval)?;
        }

        Ok(())
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

    /// Returns the raw string value for a parameter.
    ///
    /// Mirrors the JS helper `getString` defined in `packages/remote-config/src/api.ts`.
    pub fn get_string(&self, key: &str) -> String {
        self.get_value(key).as_string()
    }

    /// Returns the value interpreted as a boolean.
    ///
    /// Maps to the JS helper `getBoolean`.
    pub fn get_boolean(&self, key: &str) -> bool {
        self.get_value(key).as_bool()
    }

    /// Returns the value interpreted as a number.
    ///
    /// Maps to the JS helper `getNumber`.
    pub fn get_number(&self, key: &str) -> f64 {
        self.get_value(key).as_number()
    }

    /// Returns a value wrapper that exposes typed accessors and the source of the parameter.
    pub fn get_value(&self, key: &str) -> RemoteConfigValue {
        if let Some(value) = self.inner.values.lock().unwrap().get(key).cloned() {
            return RemoteConfigValue::new(RemoteConfigValueSource::Remote, value);
        }
        if let Some(value) = self.inner.defaults.lock().unwrap().get(key).cloned() {
            return RemoteConfigValue::new(RemoteConfigValueSource::Default, value);
        }
        RemoteConfigValue::static_value()
    }

    /// Returns the union of default and active configs, with active values taking precedence.
    pub fn get_all(&self) -> HashMap<String, RemoteConfigValue> {
        let defaults = self.inner.defaults.lock().unwrap().clone();
        let values = self.inner.values.lock().unwrap().clone();

        let mut all = HashMap::new();
        for (key, value) in defaults {
            all.insert(
                key,
                RemoteConfigValue::new(RemoteConfigValueSource::Default, value),
            );
        }
        for (key, value) in values {
            all.insert(
                key,
                RemoteConfigValue::new(RemoteConfigValueSource::Remote, value),
            );
        }
        all
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
    use crate::app::{FirebaseApp, FirebaseAppSettings, FirebaseOptions};
    use crate::remote_config::settings::{
        RemoteConfigSettingsUpdate, DEFAULT_FETCH_TIMEOUT_MILLIS,
        DEFAULT_MINIMUM_FETCH_INTERVAL_MILLIS,
    };

    fn remote_config(app: FirebaseApp) -> Arc<RemoteConfig> {
        get_remote_config(Some(app)).unwrap()
    }

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
        let rc = remote_config(app);
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
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([(String::from("flag"), String::from("off"))]));
        rc.fetch().unwrap();
        rc.activate().unwrap();
        assert!(!rc.activate().unwrap());
    }

    #[test]
    fn get_value_reports_default_source_prior_to_activation() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([(
            String::from("feature"),
            String::from("true"),
        )]));

        let value = rc.get_value("feature");
        assert_eq!(value.source(), RemoteConfigValueSource::Default);
        assert!(value.as_bool());
    }

    #[test]
    fn get_value_reports_remote_source_after_activation() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([(
            String::from("feature"),
            String::from("true"),
        )]));
        rc.fetch().unwrap();
        rc.activate().unwrap();

        let value = rc.get_value("feature");
        assert_eq!(value.source(), RemoteConfigValueSource::Remote);
        assert!(value.as_bool());
    }

    #[test]
    fn get_number_handles_invalid_values() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([(
            String::from("rate"),
            String::from("not-a-number"),
        )]));

        assert_eq!(rc.get_number("rate"), 0.0);
        assert_eq!(rc.get_number("missing"), 0.0);
    }

    #[test]
    fn get_all_merges_defaults_and_remote_values() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = remote_config(app);
        rc.set_defaults(HashMap::from([
            (String::from("feature"), String::from("true")),
            (String::from("secondary"), String::from("value")),
        ]));
        rc.fetch().unwrap();
        rc.activate().unwrap();
        rc.set_defaults(HashMap::from([
            (String::from("feature"), String::from("false")),
            (String::from("secondary"), String::from("value")),
            (String::from("fallback"), String::from("present")),
        ]));

        let all = rc.get_all();
        assert_eq!(all.len(), 3);
        assert_eq!(all["feature"].source(), RemoteConfigValueSource::Remote);
        assert_eq!(all["feature"].as_bool(), true);
        assert_eq!(all["secondary"].source(), RemoteConfigValueSource::Remote);
        assert_eq!(all["fallback"].source(), RemoteConfigValueSource::Default);
    }

    #[test]
    fn missing_key_returns_static_value() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = remote_config(app);

        let value = rc.get_value("not-present");
        assert_eq!(value.source(), RemoteConfigValueSource::Static);
        assert_eq!(value.as_string(), "");
        assert!(!value.as_bool());
        assert_eq!(value.as_number(), 0.0);
    }

    #[test]
    fn settings_defaults_match_js_constants() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = remote_config(app);

        let settings = rc.settings();
        assert_eq!(
            settings.fetch_timeout_millis(),
            DEFAULT_FETCH_TIMEOUT_MILLIS
        );
        assert_eq!(
            settings.minimum_fetch_interval_millis(),
            DEFAULT_MINIMUM_FETCH_INTERVAL_MILLIS
        );
    }

    #[test]
    fn set_config_settings_updates_values() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = remote_config(app);

        rc.set_config_settings(RemoteConfigSettingsUpdate {
            fetch_timeout_millis: Some(90_000),
            minimum_fetch_interval_millis: Some(3_600_000),
        })
        .unwrap();

        let settings = rc.settings();
        assert_eq!(settings.fetch_timeout_millis(), 90_000);
        assert_eq!(settings.minimum_fetch_interval_millis(), 3_600_000);
    }

    #[test]
    fn set_config_settings_rejects_zero_timeout() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let rc = remote_config(app);

        let result = rc.set_config_settings(RemoteConfigSettingsUpdate {
            fetch_timeout_millis: Some(0),
            minimum_fetch_interval_millis: None,
        });

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code_str(),
            crate::remote_config::error::RemoteConfigErrorCode::InvalidArgument.as_str()
        );
    }
}
