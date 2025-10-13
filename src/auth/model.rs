use crate::app::FirebaseApp;
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::token_manager::{TokenManager, TokenUpdate};
use crate::util::PartialObserver;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    pub uid: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub photo_url: Option<String>,
    pub provider_id: String,
}

#[derive(Clone, Debug)]
pub struct User {
    app: FirebaseApp,
    info: UserInfo,
    email_verified: bool,
    is_anonymous: bool,
    token_manager: TokenManager,
}

impl User {
    pub fn new(app: FirebaseApp, info: UserInfo) -> Self {
        Self {
            app,
            info,
            email_verified: false,
            is_anonymous: false,
            token_manager: TokenManager::default(),
        }
    }

    pub fn app(&self) -> &FirebaseApp {
        &self.app
    }

    pub fn is_anonymous(&self) -> bool {
        self.is_anonymous
    }

    pub fn set_anonymous(&mut self, anonymous: bool) {
        self.is_anonymous = anonymous;
    }

    pub fn uid(&self) -> &str {
        &self.info.uid
    }

    pub fn email_verified(&self) -> bool {
        self.email_verified
    }

    pub fn refresh_token(&self) -> Option<String> {
        self.token_manager.refresh_token()
    }

    pub fn get_id_token(&self, _force_refresh: bool) -> AuthResult<String> {
        self.token_manager
            .access_token()
            .ok_or_else(|| AuthError::InvalidCredential("Missing ID token".into()))
    }

    pub fn token_manager(&self) -> &TokenManager {
        &self.token_manager
    }

    pub fn update_tokens(
        &self,
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_in: Option<Duration>,
    ) {
        let update = TokenUpdate::new(access_token, refresh_token, expires_in);
        self.token_manager.update(update);
    }

    pub fn info(&self) -> &UserInfo {
        &self.info
    }
}

#[derive(Clone, Debug)]
pub struct UserCredential {
    pub user: Arc<User>,
    pub provider_id: Option<String>,
    pub operation_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuthCredential {
    pub provider_id: String,
    pub sign_in_method: String,
    pub token_response: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub api_key: Option<String>,
    pub identity_toolkit_endpoint: Option<String>,
    pub secure_token_endpoint: Option<String>,
}

#[derive(Clone)]
pub struct EmailAuthProvider;

impl EmailAuthProvider {
    pub const PROVIDER_ID: &'static str = "password";

    pub fn credential(email: &str, password: &str) -> AuthCredential {
        AuthCredential {
            provider_id: Self::PROVIDER_ID.to_string(),
            sign_in_method: Self::PROVIDER_ID.to_string(),
            token_response: json!({
                "email": email,
                "password": password,
                "returnSecureToken": true,
            }),
        }
    }
}

#[derive(Default)]
pub struct AuthStateListeners {
    observers: Mutex<Vec<PartialObserver<Arc<User>>>>,
}

impl AuthStateListeners {
    pub fn add_observer(&self, observer: PartialObserver<Arc<User>>) {
        self.observers.lock().unwrap().push(observer);
    }

    pub fn notify(&self, user: Arc<User>) {
        for observer in self.observers.lock().unwrap().iter() {
            if let Some(next) = observer.next.clone() {
                next(&user);
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SignInWithPasswordRequest {
    pub email: String,
    pub password: String,
    #[serde(rename = "returnSecureToken")]
    pub return_secure_token: bool,
}

#[derive(Debug, Deserialize)]
pub struct SignInWithPasswordResponse {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "localId")]
    pub local_id: String,
    pub email: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: String,
}

#[derive(Debug, Serialize)]
pub struct SignUpRequest {
    pub email: String,
    pub password: String,
    #[serde(rename = "returnSecureToken")]
    pub return_secure_token: bool,
}

#[derive(Debug, Deserialize)]
pub struct SignUpResponse {
    #[serde(rename = "idToken")]
    pub id_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "localId")]
    pub local_id: String,
    pub email: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<String>,
}
