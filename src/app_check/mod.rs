#![doc = include_str!("README.md")]
pub mod api;
mod errors;
mod interop;
mod logger;
#[cfg(feature = "wasm-web")]
mod persistence;
mod providers;
mod refresher;
mod state;
#[cfg(feature = "firestore")]
mod token_provider;
mod types;

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

#[doc(inline)]
pub use types::{
    AppCheck, AppCheckInternalListener, AppCheckOptions, AppCheckProvider, AppCheckToken,
    AppCheckTokenListener, AppCheckTokenResult, ListenerHandle, ListenerType,
    APP_CHECK_COMPONENT_NAME, APP_CHECK_INTERNAL_COMPONENT_NAME,
};
