use std::sync::{Arc, LazyLock, Mutex};

use crate::app::FirebaseApp;
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
use crate::app_check::errors::AppCheckError;
use crate::app_check::errors::AppCheckResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecaptchaFlow {
    V3,
    Enterprise,
}

#[derive(Clone, Debug)]
pub struct RecaptchaTokenDetails {
    pub token: String,
    pub succeeded: bool,
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait RecaptchaDriver: Send + Sync {
    fn initialize(&self, app: &FirebaseApp, site_key: &str, flow: RecaptchaFlow);
    async fn get_token(&self, app: &FirebaseApp) -> AppCheckResult<RecaptchaTokenDetails>;
}

static DRIVER_OVERRIDE: LazyLock<Mutex<Option<Arc<dyn RecaptchaDriver + Send + Sync>>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
static DEFAULT_DRIVER: LazyLock<Arc<dyn RecaptchaDriver + Send + Sync>> = LazyLock::new(|| {
    Arc::new(web::WebRecaptchaDriver::new()) as Arc<dyn RecaptchaDriver + Send + Sync>
});

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
static DEFAULT_DRIVER: LazyLock<Arc<dyn RecaptchaDriver + Send + Sync>> = LazyLock::new(|| {
    Arc::new(UnsupportedRecaptchaDriver) as Arc<dyn RecaptchaDriver + Send + Sync>
});

fn driver() -> Arc<dyn RecaptchaDriver + Send + Sync> {
    DRIVER_OVERRIDE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| DEFAULT_DRIVER.clone())
}

pub(crate) fn initialize(app: &FirebaseApp, site_key: &str, flow: RecaptchaFlow) {
    driver().initialize(app, site_key, flow);
}

pub(crate) async fn get_token(app: &FirebaseApp) -> AppCheckResult<RecaptchaTokenDetails> {
    driver().get_token(app).await
}

#[cfg(test)]
pub(crate) fn set_driver_override(driver: Arc<dyn RecaptchaDriver + Send + Sync>) {
    *DRIVER_OVERRIDE.lock().unwrap() = Some(driver);
}

#[cfg(test)]
pub(crate) fn clear_driver_override() {
    *DRIVER_OVERRIDE.lock().unwrap() = None;
}

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
struct UnsupportedRecaptchaDriver;

#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl RecaptchaDriver for UnsupportedRecaptchaDriver {
    fn initialize(&self, _app: &FirebaseApp, _site_key: &str, _flow: RecaptchaFlow) {}

    async fn get_token(&self, _app: &FirebaseApp) -> AppCheckResult<RecaptchaTokenDetails> {
        Err(AppCheckError::RecaptchaError {
            message: Some(
                "ReCAPTCHA providers require the `wasm-web` feature and WebAssembly target".into(),
            ),
        })
    }
}

#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]
mod web {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use futures::channel::oneshot;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    use super::*;

    use crate::app_check::errors::{AppCheckError, AppCheckResult};
    use crate::platform::runtime;

    #[derive(Clone)]
    pub struct WebRecaptchaDriver {
        state: Arc<WebState>,
    }

    #[derive(Default)]
    struct WebState {
        apps: Mutex<HashMap<String, Arc<AppRecaptchaState>>>,
    }

    struct AppRecaptchaState {
        site_key: Mutex<String>,
        flow: Mutex<RecaptchaFlow>,
        widget_id: Mutex<Option<String>>,
        initialized: AtomicBool,
        initializing: AtomicBool,
        succeeded: AtomicBool,
    }

    impl WebRecaptchaDriver {
        pub fn new() -> Self {
            Self {
                state: Arc::new(WebState::default()),
            }
        }

        fn app_state(&self, app: &FirebaseApp) -> Arc<AppRecaptchaState> {
            let app_name = app.name().to_owned();
            let mut guard = self.state.apps.lock().unwrap();
            guard
                .entry(app_name.clone())
                .or_insert_with(|| {
                    Arc::new(AppRecaptchaState {
                        site_key: Mutex::new(String::new()),
                        flow: Mutex::new(RecaptchaFlow::V3),
                        widget_id: Mutex::new(None),
                        initialized: AtomicBool::new(false),
                        initializing: AtomicBool::new(false),
                        succeeded: AtomicBool::new(false),
                    })
                })
                .clone()
        }

        fn ensure_initialized(&self, app: FirebaseApp, site_key: String, flow: RecaptchaFlow) {
            let state = self.app_state(&app);
            {
                *state.site_key.lock().unwrap() = site_key.clone();
                *state.flow.lock().unwrap() = flow;
            }

            if state.initialized.load(Ordering::SeqCst)
                || state
                    .initializing
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
            {
                return;
            }

            let state_for_task = state.clone();
            let app_clone = app.clone();
            runtime::spawn_detached(async move {
                let result =
                    initialize_app_state(&app_clone, state_for_task.clone(), site_key, flow).await;
                if result.is_err() {
                    state_for_task.initialized.store(false, Ordering::SeqCst);
                } else {
                    state_for_task.initialized.store(true, Ordering::SeqCst);
                }
                state_for_task.initializing.store(false, Ordering::SeqCst);
            });
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl RecaptchaDriver for WebRecaptchaDriver {
        fn initialize(&self, app: &FirebaseApp, site_key: &str, flow: RecaptchaFlow) {
            self.ensure_initialized(app.clone(), site_key.to_owned(), flow);
        }

        async fn get_token(&self, app: &FirebaseApp) -> AppCheckResult<RecaptchaTokenDetails> {
            let state = self.app_state(app);

            while !state.initialized.load(Ordering::SeqCst) {
                if !state.initializing.load(Ordering::SeqCst) {
                    return Err(AppCheckError::RecaptchaError {
                        message: Some("reCAPTCHA failed to initialize".into()),
                    });
                }
                runtime::yield_now().await;
            }

            let flow = *state.flow.lock().unwrap();
            let grecaptcha = get_grecaptcha(flow).ok_or(AppCheckError::RecaptchaError {
                message: Some("reCAPTCHA scripts have not been loaded".into()),
            })?;

            let widget_id =
                state
                    .widget_id
                    .lock()
                    .unwrap()
                    .clone()
                    .ok_or(AppCheckError::RecaptchaError {
                        message: Some("reCAPTCHA widget not rendered".into()),
                    })?;

            state.succeeded.store(false, Ordering::SeqCst);
            let token = execute_token(&grecaptcha, &widget_id).await?;
            let succeeded = state.succeeded.load(Ordering::SeqCst);

            Ok(RecaptchaTokenDetails { token, succeeded })
        }
    }

    async fn initialize_app_state(
        app: &FirebaseApp,
        state: Arc<AppRecaptchaState>,
        site_key: String,
        flow: RecaptchaFlow,
    ) -> AppCheckResult<()> {
        let grecaptcha = match get_grecaptcha(flow) {
            Some(g) => g,
            None => {
                load_script(flow).await?;
                get_grecaptcha(flow).ok_or(AppCheckError::RecaptchaError {
                    message: Some("Failed to load reCAPTCHA script".into()),
                })?
            }
        };

        let container_id = ensure_container(app.name())?;
        let widget_id = render_widget(grecaptcha, container_id, state.clone(), site_key).await?;
        *state.widget_id.lock().unwrap() = Some(widget_id);
        Ok(())
    }

    fn ensure_container(app_name: &str) -> AppCheckResult<String> {
        let window = web_sys::window().ok_or(AppCheckError::RecaptchaError {
            message: Some("Window not available".into()),
        })?;
        let document = window.document().ok_or(AppCheckError::RecaptchaError {
            message: Some("Document not available".into()),
        })?;
        let container_id = format!("fire_app_check_{}", app_name);
        if document.get_element_by_id(&container_id).is_none() {
            let element = document
                .create_element("div")
                .map_err(|err| AppCheckError::RecaptchaError {
                    message: Some(format!("Failed to create container: {err:?}")),
                })?
                .dyn_into::<web_sys::HtmlDivElement>()
                .map_err(|_| AppCheckError::RecaptchaError {
                    message: Some("Container element is not a div".into()),
                })?;
            element.set_id(&container_id);
            element
                .style()
                .set_property("display", "none")
                .map_err(|err| AppCheckError::RecaptchaError {
                    message: Some(format!("Failed to hide container: {err:?}")),
                })?;
            document
                .body()
                .ok_or(AppCheckError::RecaptchaError {
                    message: Some("Document body not available".into()),
                })?
                .append_child(&element)
                .map_err(|err| AppCheckError::RecaptchaError {
                    message: Some(format!("Failed to append container: {err:?}")),
                })?;
        }
        Ok(container_id)
    }

    async fn render_widget(
        grecaptcha: js_sys::Object,
        container_id: String,
        state: Arc<AppRecaptchaState>,
        site_key: String,
    ) -> AppCheckResult<String> {
        let (sender, receiver) = oneshot::channel::<Result<String, AppCheckError>>();
        let sender = Rc::new(RefCell::new(Some(sender)));
        let ready_sender = sender.clone();
        let container_clone = container_id.clone();
        let grecaptcha_clone = grecaptcha.clone();
        let site_key_clone = site_key.clone();
        let ready = Closure::wrap(Box::new(move || {
            let result = render_invisible_widget(
                &grecaptcha_clone,
                &container_clone,
                &site_key_clone,
                state.clone(),
            );
            if let Some(tx) = ready_sender.borrow_mut().take() {
                let _ = tx.send(result);
            }
        }) as Box<dyn FnMut()>);

        call_ready(&grecaptcha, &ready)?;
        ready.forget();

        let result = receiver.await.map_err(|_| AppCheckError::RecaptchaError {
            message: Some("reCAPTCHA ready callback dropped".into()),
        })?;
        result
    }

    fn render_invisible_widget(
        grecaptcha: &js_sys::Object,
        container_id: &str,
        site_key: &str,
        state: Arc<AppRecaptchaState>,
    ) -> Result<String, AppCheckError> {
        use js_sys::{Function, Object};
        use wasm_bindgen::prelude::*;

        let options = Object::new();
        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("sitekey"),
            &JsValue::from_str(site_key),
        )
        .map_err(|err| AppCheckError::RecaptchaError {
            message: Some(format!("Failed to set sitekey: {err:?}")),
        })?;
        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("size"),
            &JsValue::from_str("invisible"),
        )
        .map_err(|err| AppCheckError::RecaptchaError {
            message: Some(format!("Failed to set size: {err:?}")),
        })?;

        let success_state = state.clone();
        let success = Closure::wrap(Box::new(move || {
            success_state.succeeded.store(true, Ordering::SeqCst);
        }) as Box<dyn FnMut()>);

        let error_state = state.clone();
        let error = Closure::wrap(Box::new(move || {
            error_state.succeeded.store(false, Ordering::SeqCst);
        }) as Box<dyn FnMut()>);

        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("callback"),
            success.as_ref().unchecked_ref(),
        )
        .map_err(|err| AppCheckError::RecaptchaError {
            message: Some(format!("Failed to set callback: {err:?}")),
        })?;
        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("error-callback"),
            error.as_ref().unchecked_ref(),
        )
        .map_err(|err| AppCheckError::RecaptchaError {
            message: Some(format!("Failed to set error callback: {err:?}")),
        })?;

        let render = js_sys::Reflect::get(grecaptcha, &JsValue::from_str("render"))
            .map_err(|err| AppCheckError::RecaptchaError {
                message: Some(format!("Failed to access render(): {err:?}")),
            })?
            .dyn_into::<Function>()
            .map_err(|_| AppCheckError::RecaptchaError {
                message: Some("render() is not a function".into()),
            })?;

        let widget_id = render
            .call2(
                grecaptcha,
                &JsValue::from_str(container_id),
                &options.into(),
            )
            .map_err(|err| AppCheckError::RecaptchaError {
                message: Some(format!("render() threw: {err:?}")),
            })?;

        success.forget();
        error.forget();

        widget_id.as_string().ok_or(AppCheckError::RecaptchaError {
            message: Some("render() did not return a widget id".into()),
        })
    }

    async fn execute_token(grecaptcha: &js_sys::Object, widget_id: &str) -> AppCheckResult<String> {
        use js_sys::{Function, Object, Promise};

        let execute = js_sys::Reflect::get(grecaptcha, &JsValue::from_str("execute"))
            .map_err(|err| AppCheckError::RecaptchaError {
                message: Some(format!("Failed to access execute(): {err:?}")),
            })?
            .dyn_into::<Function>()
            .map_err(|_| AppCheckError::RecaptchaError {
                message: Some("execute() is not a function".into()),
            })?;

        let options = Object::new();
        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("action"),
            &JsValue::from_str("fire_app_check"),
        )
        .map_err(|err| AppCheckError::RecaptchaError {
            message: Some(format!("Failed to set execute action: {err:?}")),
        })?;

        let promise = execute
            .call2(grecaptcha, &JsValue::from_str(widget_id), &options.into())
            .map_err(|err| AppCheckError::RecaptchaError {
                message: Some(format!("execute() threw: {err:?}")),
            })?
            .dyn_into::<Promise>()
            .map_err(|_| AppCheckError::RecaptchaError {
                message: Some("execute() did not return a Promise".into()),
            })?;

        let value = wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|err| AppCheckError::RecaptchaError {
                message: Some(js_error_message(err)),
            })?;

        value.as_string().ok_or(AppCheckError::RecaptchaError {
            message: Some("execute() promise resolved with non-string".into()),
        })
    }

    fn call_ready(
        grecaptcha: &js_sys::Object,
        callback: &Closure<dyn FnMut()>,
    ) -> Result<(), AppCheckError> {
        use js_sys::Function;

        let ready = js_sys::Reflect::get(grecaptcha, &JsValue::from_str("ready"))
            .map_err(|err| AppCheckError::RecaptchaError {
                message: Some(format!("Failed to access ready(): {err:?}")),
            })?
            .dyn_into::<Function>()
            .map_err(|_| AppCheckError::RecaptchaError {
                message: Some("ready() is not a function".into()),
            })?;

        ready
            .call1(grecaptcha, callback.as_ref().unchecked_ref())
            .map_err(|err| AppCheckError::RecaptchaError {
                message: Some(format!("ready() threw: {err:?}")),
            })?;
        Ok(())
    }

    fn get_grecaptcha(flow: RecaptchaFlow) -> Option<js_sys::Object> {
        let global = js_sys::global();
        let value = js_sys::Reflect::get(&global, &JsValue::from_str("grecaptcha")).ok()?;
        if value.is_null() || value.is_undefined() {
            return None;
        }
        if matches!(flow, RecaptchaFlow::Enterprise) {
            let enterprise = js_sys::Reflect::get(&value, &JsValue::from_str("enterprise")).ok()?;
            if enterprise.is_null() || enterprise.is_undefined() {
                None
            } else {
                Some(enterprise.unchecked_into())
            }
        } else {
            Some(value.unchecked_into())
        }
    }

    async fn load_script(flow: RecaptchaFlow) -> AppCheckResult<()> {
        let url = match flow {
            RecaptchaFlow::V3 => "https://www.google.com/recaptcha/api.js",
            RecaptchaFlow::Enterprise => "https://www.google.com/recaptcha/enterprise.js",
        };

        if let Some(document) = web_sys::window().and_then(|win| win.document()) {
            if document
                .query_selector(&format!("script[src=\"{url}\"]"))
                .ok()
                .flatten()
                .is_some()
            {
                return Ok(());
            }
        }

        let window = web_sys::window().ok_or(AppCheckError::RecaptchaError {
            message: Some("Window not available".into()),
        })?;
        let document = window.document().ok_or(AppCheckError::RecaptchaError {
            message: Some("Document not available".into()),
        })?;

        let script = document
            .create_element("script")
            .map_err(|err| AppCheckError::RecaptchaError {
                message: Some(format!("Failed to create script: {err:?}")),
            })?
            .dyn_into::<web_sys::HtmlScriptElement>()
            .map_err(|_| AppCheckError::RecaptchaError {
                message: Some("Script element has wrong type".into()),
            })?;
        script.set_src(url);

        let (sender, receiver) = oneshot::channel::<Result<(), AppCheckError>>();
        let sender = Rc::new(RefCell::new(Some(sender)));
        let success_sender = sender.clone();
        let onload = Closure::wrap(Box::new(move || {
            if let Some(tx) = success_sender.borrow_mut().take() {
                let _ = tx.send(Ok(()));
            }
        }) as Box<dyn FnMut()>);

        let error_sender = sender.clone();
        let url_string = url.to_string();
        let onerror = Closure::wrap(Box::new(move || {
            if let Some(tx) = error_sender.borrow_mut().take() {
                let _ = tx.send(Err(AppCheckError::RecaptchaError {
                    message: Some(format!("Failed to load reCAPTCHA script: {url_string}")),
                }));
            }
        }) as Box<dyn FnMut()>);

        script.set_onload(Some(onload.as_ref().unchecked_ref()));
        script.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        onload.forget();
        onerror.forget();

        if let Some(head) = document.head() {
            head.append_child(&script)
                .map_err(|err| AppCheckError::RecaptchaError {
                    message: Some(format!("Failed to append script to <head>: {err:?}")),
                })?;
        } else if let Some(body) = document.body() {
            body.append_child(&script)
                .map_err(|err| AppCheckError::RecaptchaError {
                    message: Some(format!("Failed to append script to <body>: {err:?}")),
                })?;
        } else {
            return Err(AppCheckError::RecaptchaError {
                message: Some("No <head> or <body> element found".into()),
            });
        }

        let result = receiver.await.map_err(|_| AppCheckError::RecaptchaError {
            message: Some("Script loading channel dropped".into()),
        })?;
        result
    }

    fn js_error_message(value: JsValue) -> String {
        if let Some(error) = value.dyn_ref::<js_sys::Error>() {
            format!("{}", error.message())
        } else if let Some(string) = value.as_string() {
            string
        } else {
            format!("{value:?}")
        }
    }
}
