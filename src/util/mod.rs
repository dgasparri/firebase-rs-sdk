pub mod assert;
pub mod backoff;
pub mod base64;
pub mod compat;
pub mod constants;
pub mod deep_copy;
pub mod environment;
pub mod errors;
pub mod formatters;
pub mod json;
pub mod jwt;
pub mod obj;
pub mod sha1;
pub mod subscribe;

pub use assert::{assert, assertion_error};
pub use backoff::{calculate_backoff_millis, BackoffConfig, MAX_BACKOFF_MILLIS, RANDOM_FACTOR};
pub use base64::{
    base64_decode, base64_decode_bytes, base64_encode, base64_url_encode,
    base64_url_encode_trimmed, DecodeBase64Error,
};
pub use compat::{get_compat_delegate, get_modular_instance, Compat};
pub use constants::CONSTANTS;
pub use deep_copy::{deep_copy, deep_extend};
pub use environment::{
    are_cookies_enabled, get_user_agent, is_browser, is_browser_extension, is_cloudflare_worker,
    is_electron, is_indexed_db_available, is_mobile_cordova, is_node, is_react_native, is_safari,
    is_safari_or_webkit, is_uwp, is_web_worker,
};
pub use errors::{ErrorData, ErrorFactory, ErrorMap, FirebaseError};
pub use formatters::ordinal;
pub use json::{json_eval, stringify};
pub use jwt::{
    decode_jwt, is_admin_token, is_valid_format as jwt_is_valid_format,
    is_valid_timestamp as jwt_is_valid_timestamp, issued_at_time as jwt_issued_at_time,
    DecodedToken,
};
pub use obj::{deep_equal, is_empty, map_values};
pub use sha1::{sha1_digest, sha1_hex};
pub use subscribe::{PartialObserver, Unsubscribe};
