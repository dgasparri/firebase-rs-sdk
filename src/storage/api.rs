use std::sync::{Arc, LazyLock};

use crate::app::FirebaseApp;
use crate::app::{get_app, get_provider, register_component, SDK_VERSION};
use crate::component::types::{
    ComponentError, DynService, InstanceFactoryOptions, InstantiationMode,
};
use crate::component::{Component, ComponentType};
use crate::storage::constants::STORAGE_TYPE;
use crate::storage::error::{internal_error, StorageResult};
use crate::storage::reference::StorageReference;
use crate::storage::service::FirebaseStorageImpl;
use crate::storage::util::is_url;

static STORAGE_COMPONENT_REGISTERED: LazyLock<()> = LazyLock::new(|| {
    let component = Component::new(
        STORAGE_TYPE,
        Arc::new(storage_factory),
        ComponentType::Public,
    )
    .with_instantiation_mode(InstantiationMode::Lazy)
    .with_multiple_instances(true);
    let _ = register_component(component);
});

fn storage_factory(
    container: &crate::component::ComponentContainer,
    options: InstanceFactoryOptions,
) -> Result<DynService, ComponentError> {
    let app = container.root_service::<FirebaseApp>().ok_or_else(|| {
        ComponentError::InitializationFailed {
            name: STORAGE_TYPE.to_string(),
            reason: "Firebase app not attached to component container".to_string(),
        }
    })?;

    let auth_provider = container.get_provider("auth-internal");
    let app_check_provider = container.get_provider("app-check-internal");

    let storage = FirebaseStorageImpl::new(
        (*app).clone(),
        auth_provider,
        app_check_provider,
        options.instance_identifier.clone(),
        Some(SDK_VERSION.to_string()),
    )
    .map_err(|err| ComponentError::InitializationFailed {
        name: STORAGE_TYPE.to_string(),
        reason: err.to_string(),
    })?;

    Ok(Arc::new(storage) as DynService)
}

fn ensure_registered() {
    LazyLock::force(&STORAGE_COMPONENT_REGISTERED);
}

pub fn register_storage_component() {
    ensure_registered();
}

pub async fn get_storage_for_app(
    app: Option<FirebaseApp>,
    bucket_url: Option<&str>,
) -> StorageResult<Arc<FirebaseStorageImpl>> {
    ensure_registered();
    let app = match app {
        Some(app) => app,
        None => get_app(None)
            .await
            .map_err(|err| internal_error(err.to_string()))?,
    };

    let provider = get_provider(&app, STORAGE_TYPE);
    let storage = provider
        .get_immediate_with_options::<FirebaseStorageImpl>(bucket_url, false)
        .map_err(|err| internal_error(err.to_string()))?
        .ok_or_else(|| internal_error("Storage component did not return an instance"))?;

    Ok(storage)
}

pub fn storage_ref_from_storage(
    storage: &FirebaseStorageImpl,
    path_or_url: Option<&str>,
) -> StorageResult<StorageReference> {
    storage.reference_from_path(path_or_url)
}

pub fn storage_ref_from_reference(
    reference: &StorageReference,
    path: Option<&str>,
) -> StorageResult<StorageReference> {
    match path {
        Some(segment) if is_url(segment) => {
            // Mirrors JS behaviour: URLs must be paired with a Storage instance, not a reference.
            Err(internal_error(
                "Use storage_ref_from_storage for URL-based references",
            ))
        }
        Some(segment) => Ok(reference.child(segment)),
        None => Ok(reference.clone()),
    }
}

pub fn connect_storage_emulator(
    storage: &FirebaseStorageImpl,
    host: &str,
    port: u16,
    mock_user_token: Option<String>,
) -> StorageResult<()> {
    storage.connect_emulator(host, port, mock_user_token)
}

pub fn delete_storage_instance(storage: &FirebaseStorageImpl) {
    let bucket = storage.bucket();
    if bucket.is_some() {
        // Components currently lack explicit cleanup hooks; placeholder for parity.
    }
}
