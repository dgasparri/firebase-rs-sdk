//! Lightweight IndexedDB helpers shared across browser-facing modules.

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
mod wasm {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{
        DomStringList, Event, IdbDatabase, IdbOpenDbRequest, IdbRequest, IdbTransactionMode,
        IdbVersionChangeEvent,
    };

    #[derive(Debug)]
    pub enum IndexedDbError {
        Unsupported(&'static str),
        Operation(String),
    }

    impl std::fmt::Display for IndexedDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                IndexedDbError::Unsupported(msg) => write!(f, "IndexedDB unsupported: {msg}"),
                IndexedDbError::Operation(msg) => write!(f, "IndexedDB error: {msg}"),
            }
        }
    }

    impl std::error::Error for IndexedDbError {}

    pub type IndexedDbResult<T> = Result<T, IndexedDbError>;

    const UNSUPPORTED: &str = "IndexedDB APIs are not available in this environment";

    /// Opens (or creates) an IndexedDB database, ensuring that the provided object store exists.
    pub async fn open_database_with_store(
        name: &str,
        version: u32,
        store: &str,
    ) -> IndexedDbResult<IdbDatabase> {
        let window = web_sys::window().ok_or(IndexedDbError::Unsupported(UNSUPPORTED))?;
        let factory = window
            .indexed_db()
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?
            .ok_or(IndexedDbError::Unsupported(UNSUPPORTED))?;
        let request = factory
            .open_with_u32(name, version)
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;

        let store_name = store.to_owned();
        let upgrade_handler = Closure::wrap(Box::new(move |event: IdbVersionChangeEvent| {
            if let Some(target) = event.target() {
                if let Ok(open_request) = target.dyn_into::<IdbOpenDbRequest>() {
                    if let Ok(result) = open_request.result() {
                        if let Ok(db) = result.dyn_into::<IdbDatabase>() {
                            ensure_store_exists(&db, &store_name);
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        request.set_onupgradeneeded(Some(upgrade_handler.as_ref().unchecked_ref()));
        upgrade_handler.forget();

        let db_js = JsFuture::from(request_to_future(clone_as_idb_request(&request)))
            .await
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        let db: IdbDatabase = db_js
            .dyn_into()
            .map_err(|_| IndexedDbError::Operation("Failed to acquire database handle".into()))?;
        Ok(db)
    }

    /// Reads a UTF-8 string value from the specified store and key.
    pub async fn get_string(
        db: &IdbDatabase,
        store: &str,
        key: &str,
    ) -> IndexedDbResult<Option<String>> {
        let tx = db
            .transaction_with_str_and_mode(store, IdbTransactionMode::Readonly)
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        let object_store = tx
            .object_store(store)
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        let request = object_store
            .get(&JsValue::from_str(key))
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        let result = JsFuture::from(request_to_future(request))
            .await
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        if result.is_undefined() || result.is_null() {
            Ok(None)
        } else if let Some(value) = result.as_string() {
            Ok(Some(value))
        } else {
            Err(IndexedDbError::Operation(
                "Stored value is not a string".into(),
            ))
        }
    }

    /// Writes a UTF-8 string value into the specified store/key.
    pub async fn put_string(
        db: &IdbDatabase,
        store: &str,
        key: &str,
        value: &str,
    ) -> IndexedDbResult<()> {
        let tx = db
            .transaction_with_str_and_mode(store, IdbTransactionMode::Readwrite)
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        let object_store = tx
            .object_store(store)
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        let request = object_store
            .put_with_key(&JsValue::from_str(value), &JsValue::from_str(key))
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        JsFuture::from(request_to_future(request))
            .await
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        Ok(())
    }

    /// Deletes the value stored under the given key.
    pub async fn delete_key(db: &IdbDatabase, store: &str, key: &str) -> IndexedDbResult<()> {
        let tx = db
            .transaction_with_str_and_mode(store, IdbTransactionMode::Readwrite)
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        let object_store = tx
            .object_store(store)
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        let request = object_store
            .delete(&JsValue::from_str(key))
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        JsFuture::from(request_to_future(request))
            .await
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        Ok(())
    }

    /// Deletes the entire database. Useful for tests.
    pub async fn delete_database(name: &str) -> IndexedDbResult<()> {
        let window = web_sys::window().ok_or(IndexedDbError::Unsupported(UNSUPPORTED))?;
        let factory = window
            .indexed_db()
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?
            .ok_or(IndexedDbError::Unsupported(UNSUPPORTED))?;
        let request = factory
            .delete_database(name)
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        JsFuture::from(request_to_future(clone_as_idb_request(&request)))
            .await
            .map_err(|err| IndexedDbError::Operation(js_value_to_string(&err)))?;
        Ok(())
    }

    fn ensure_store_exists(db: &IdbDatabase, store: &str) {
        let existing = db.object_store_names();
        if !dom_string_list_contains(&existing, store) {
            let _ = db.create_object_store(store);
        }
    }

    fn dom_string_list_contains(list: &DomStringList, target: &str) -> bool {
        for idx in 0..list.length() {
            if let Some(value) = list.item(idx) {
                if value == target {
                    return true;
                }
            }
        }
        false
    }

    fn request_to_future(request: IdbRequest) -> js_sys::Promise {
        let success_request = request.clone();
        let error_request = request.clone();
        js_sys::Promise::new(&mut move |resolve, reject| {
            let resolve_fn = resolve.clone();
            let reject_for_success = reject.clone();
            let success_request_clone = success_request.clone();
            let success = Closure::once(
                Box::new(move |_event: Event| match success_request_clone.result() {
                    Ok(result) => {
                        let _ = resolve_fn.call1(&JsValue::UNDEFINED, &result);
                    }
                    Err(err) => {
                        let _ = reject_for_success.call1(&JsValue::UNDEFINED, &err);
                    }
                }) as Box<dyn FnMut(_)>,
            );
            request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
            success.forget();

            let reject_fn = reject.clone();
            let error_request_clone = error_request.clone();
            let error = Closure::once(Box::new(move |_event: Event| {
                match error_request_clone.error() {
                    Ok(Some(err)) => {
                        let _ = reject_fn.call1(&JsValue::UNDEFINED, &err);
                    }
                    Ok(None) => {
                        let _ = reject_fn.call1(&JsValue::UNDEFINED, &JsValue::NULL);
                    }
                    Err(js_err) => {
                        let _ = reject_fn.call1(&JsValue::UNDEFINED, &js_err);
                    }
                }
            }) as Box<dyn FnMut(_)>);
            request.set_onerror(Some(error.as_ref().unchecked_ref()));
            error.forget();
        })
    }

    fn clone_as_idb_request(request: &IdbOpenDbRequest) -> IdbRequest {
        request.clone().unchecked_into::<IdbRequest>()
    }

    fn js_value_to_string(value: &JsValue) -> String {
        if let Some(exception) = value.dyn_ref::<web_sys::DomException>() {
            format!("{}: {}", exception.name(), exception.message())
        } else if let Some(text) = value.as_string() {
            text
        } else {
            format!("{:?}", value)
        }
    }

    pub use IndexedDbError as Error;
}

#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
pub use wasm::*;

#[cfg(not(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
)))]
mod stub {

    #[derive(Debug)]
    pub enum IndexedDbError {
        Unsupported,
    }

    impl std::fmt::Display for IndexedDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "IndexedDB not supported on this target")
        }
    }

    impl std::error::Error for IndexedDbError {}

    pub type IndexedDbResult<T> = std::result::Result<T, IndexedDbError>;

    pub type IdbDatabase = ();

    pub async fn open_database_with_store(
        _name: &str,
        _version: u32,
        _store: &str,
    ) -> IndexedDbResult<IdbDatabase> {
        Err(IndexedDbError::Unsupported)
    }

    pub async fn get_string(
        _db: &IdbDatabase,
        _store: &str,
        _key: &str,
    ) -> IndexedDbResult<Option<String>> {
        Err(IndexedDbError::Unsupported)
    }

    pub async fn put_string(
        _db: &IdbDatabase,
        _store: &str,
        _key: &str,
        _value: &str,
    ) -> IndexedDbResult<()> {
        Err(IndexedDbError::Unsupported)
    }

    pub async fn delete_key(_db: &IdbDatabase, _store: &str, _key: &str) -> IndexedDbResult<()> {
        Err(IndexedDbError::Unsupported)
    }

    pub async fn delete_database(_name: &str) -> IndexedDbResult<()> {
        Err(IndexedDbError::Unsupported)
    }

    pub use IndexedDbError as Error;
}

#[cfg(not(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
)))]
pub use stub::*;
