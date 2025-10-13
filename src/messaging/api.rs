use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use crate::app;
use crate::app::FirebaseApp;
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::messaging::constants::MESSAGING_COMPONENT_NAME;
use crate::messaging::error::{
    internal_error, invalid_argument, token_deletion_failed, MessagingResult,
};

#[derive(Clone, Debug)]
pub struct Messaging {
    inner: Arc<MessagingInner>,
}

#[derive(Debug)]
struct MessagingInner {
    app: FirebaseApp,
    tokens: Mutex<HashMap<String, String>>,
}

impl Messaging {
    fn new(app: FirebaseApp) -> Self {
        Self {
            inner: Arc::new(MessagingInner {
                app,
                tokens: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.inner.app
    }

    pub fn request_permission(&self) -> MessagingResult<bool> {
        // Minimal stub always grants permission.
        Ok(true)
    }

    pub fn get_token(&self, vapid_key: Option<&str>) -> MessagingResult<String> {
        if let Some(key) = vapid_key {
            if key.trim().is_empty() {
                return Err(invalid_argument("VAPID key must not be empty"));
            }
        }
        let mut tokens = self.inner.tokens.lock().unwrap();
        let entry = tokens
            .entry(self.inner.app.name().to_string())
            .or_insert_with(generate_token);
        Ok(entry.clone())
    }

    pub fn delete_token(&self) -> MessagingResult<bool> {
        let mut tokens = self.inner.tokens.lock().unwrap();
        if tokens.remove(self.inner.app.name()).is_some() {
            Ok(true)
        } else {
            Err(token_deletion_failed("No token stored for this app"))
        }
    }
}

fn generate_token() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(32)
        .collect()
}

static MESSAGING_COMPONENT: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        MESSAGING_COMPONENT_NAME,
        Arc::new(messaging_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy);
    let _ = app::registry::register_component(component);
});

fn messaging_factory(
    container: &crate::component::ComponentContainer,
    _options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: MESSAGING_COMPONENT_NAME.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;
    let messaging = Messaging::new((*app).clone());
    Ok(Arc::new(messaging) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&MESSAGING_COMPONENT);
}

pub fn register_messaging_component() {
    ensure_registered();
}

pub fn get_messaging(app: Option<FirebaseApp>) -> MessagingResult<Arc<Messaging>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => crate::app::api::get_app(None).map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = app::registry::get_provider(&app, MESSAGING_COMPONENT_NAME);
    if let Some(messaging) = provider.get_immediate::<Messaging>() {
        Ok(messaging)
    } else {
        provider
            .initialize::<Messaging>(serde_json::Value::Null, None)
            .map_err(|err| internal_error(err.to_string()))
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
                "messaging-{}",
                COUNTER.fetch_add(1, Ordering::SeqCst)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn token_is_stable_until_deleted() {
        let options = FirebaseOptions {
            project_id: Some("project".into()),
            ..Default::default()
        };
        let app = initialize_app(options, Some(unique_settings())).unwrap();
        let messaging = get_messaging(Some(app)).unwrap();
        assert!(messaging.request_permission().unwrap());
        let token1 = messaging.get_token(None).unwrap();
        let token2 = messaging.get_token(None).unwrap();
        assert_eq!(token1, token2);
        messaging.delete_token().unwrap();
        let token3 = messaging.get_token(None).unwrap();
        assert_ne!(token1, token3);
    }
}
