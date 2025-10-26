#![cfg(all(target_arch = "wasm32", feature = "wasm-web"))]

use firebase_rs_sdk::app::api::{delete_app, initialize_app};
use firebase_rs_sdk::app::{AppError, FirebaseApp, FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::app_check::AppCheckOptions;
use firebase_rs_sdk::app_check::{
    custom_provider, get_limited_use_token, get_token, initialize_app_check, token_with_ttl,
};
use firebase_rs_sdk::auth::api::Auth;
use std::time::Duration;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

async fn init_test_app(name: &str) -> FirebaseApp {
    let mut options = FirebaseOptions::default();
    options.api_key = Some("wasm-test-key".into());
    options.project_id = Some("wasm-test-project".into());
    let settings = FirebaseAppSettings {
        name: Some(name.into()),
        automatic_data_collection_enabled: Some(true),
    };

    initialize_app(options, Some(settings))
        .await
        .expect("initialize app")
}

#[wasm_bindgen_test(async)]
async fn initialize_app_requires_options() {
    let result = initialize_app(
        FirebaseOptions::default(),
        Some(FirebaseAppSettings::default()),
    )
    .await;
    assert!(matches!(result, Err(AppError::NoOptions)));
}

#[wasm_bindgen_test(async)]
async fn auth_reports_not_supported_on_wasm() {
    let app = init_test_app("wasm-auth").await;

    let auth = Auth::new(app.clone()).expect("create auth");
    let token = auth.get_token(true).await.unwrap();
    assert!(token.is_none(), "expected auth token to be none on wasm");

    let sign_in = auth
        .sign_in_with_email_and_password("user@example.com", "password")
        .await;
    assert!(sign_in.is_err(), "expected sign in to error on wasm");

    delete_app(&app).await.expect("delete app");
}

#[wasm_bindgen_test(async)]
async fn app_check_custom_provider_produces_token() {
    let app = init_test_app("wasm-app-check").await;

    let provider = custom_provider(|| token_with_ttl("wasm-token", Duration::from_secs(60)));
    let options = AppCheckOptions::new(provider);
    let app_check = initialize_app_check(Some(app.clone()), options)
        .await
        .expect("initialize app check");

    let token = get_token(&app_check, false).await.expect("get token");
    assert_eq!(token.token, "wasm-token");

    let limited = get_limited_use_token(&app_check)
        .await
        .expect("get limited token");
    assert_eq!(limited.token, "wasm-token");

    delete_app(&app).await.expect("delete app");
}
