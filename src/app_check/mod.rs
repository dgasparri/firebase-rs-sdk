#![doc = include_str!("README.md")]
pub mod api;
mod client;
mod errors;
mod interop;
mod logger;
#[cfg(feature = "wasm-web")]
mod persistence;
mod providers;
mod recaptcha;
mod refresher;
mod state;
#[cfg(feature = "firestore")]
mod token_provider;
mod types;
mod util;

#[doc(inline)]
pub use api::{
    add_token_listener, custom_provider, get_limited_use_token, get_token, initialize_app_check,
    recaptcha_enterprise_provider, recaptcha_v3_provider, remove_token_listener,
    set_token_auto_refresh_enabled, token_with_ttl,
};

#[doc(inline)]
pub use errors::{AppCheckError, AppCheckResult};

#[doc(inline)]
pub use interop::FirebaseAppCheckInternal;

#[doc(inline)]
pub use providers::{
    CustomProvider, CustomProviderOptions, ReCaptchaEnterpriseProvider, ReCaptchaV3Provider,
};

#[cfg(feature = "firestore")]
#[doc(inline)]
pub use token_provider::{app_check_token_provider_arc, AppCheckTokenProvider};

#[allow(unused_imports)]
pub(crate) use types::box_app_check_future;

#[doc(inline)]
pub use types::{
    AppCheck, AppCheckInternalListener, AppCheckOptions, AppCheckProvider, AppCheckProviderFuture,
    AppCheckToken, AppCheckTokenListener, AppCheckTokenResult, ListenerHandle, ListenerType,
    APP_CHECK_COMPONENT_NAME, APP_CHECK_INTERNAL_COMPONENT_NAME,
};
