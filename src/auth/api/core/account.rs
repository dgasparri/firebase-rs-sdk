use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::{
    GetAccountInfoResponse, MfaEnrollmentInfo, ProviderUserInfo, SignInWithPasswordRequest,
    SignInWithPasswordResponse,
};
use crate::auth::types::{ActionCodeOperation, ActionCodeSettings};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

fn identity_toolkit_url(base: &str, path: &str, api_key: &str) -> String {
    format!("{}/{}?key={}", base.trim_end_matches('/'), path, api_key)
}

#[derive(Debug, Clone, Serialize)]
struct SendOobCodeRequest {
    #[serde(rename = "requestType")]
    request_type: String,
    #[serde(rename = "email", skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(rename = "newEmail", skip_serializing_if = "Option::is_none")]
    new_email: Option<String>,
    #[serde(rename = "idToken", skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
    #[serde(rename = "continueUrl", skip_serializing_if = "Option::is_none")]
    continue_url: Option<String>,
    #[serde(rename = "iOSBundleId", skip_serializing_if = "Option::is_none")]
    ios_bundle_id: Option<String>,
    #[serde(rename = "iosAppStoreId", skip_serializing_if = "Option::is_none")]
    ios_app_store_id: Option<String>,
    #[serde(rename = "androidPackageName", skip_serializing_if = "Option::is_none")]
    android_package_name: Option<String>,
    #[serde(rename = "androidInstallApp", skip_serializing_if = "Option::is_none")]
    android_install_app: Option<bool>,
    #[serde(
        rename = "androidMinimumVersionCode",
        skip_serializing_if = "Option::is_none"
    )]
    android_minimum_version_code: Option<String>,
    #[serde(rename = "canHandleCodeInApp", skip_serializing_if = "Option::is_none")]
    can_handle_code_in_app: Option<bool>,
    #[serde(rename = "dynamicLinkDomain", skip_serializing_if = "Option::is_none")]
    dynamic_link_domain: Option<String>,
    #[serde(rename = "linkDomain", skip_serializing_if = "Option::is_none")]
    link_domain: Option<String>,
    #[serde(rename = "captchaResp", skip_serializing_if = "Option::is_none")]
    captcha_resp: Option<String>,
    #[serde(rename = "clientType", skip_serializing_if = "Option::is_none")]
    client_type: Option<String>,
    #[serde(rename = "recaptchaVersion", skip_serializing_if = "Option::is_none")]
    recaptcha_version: Option<String>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    tenant_id: Option<String>,
    #[serde(rename = "targetProjectid", skip_serializing_if = "Option::is_none")]
    target_project_id: Option<String>,
}

impl SendOobCodeRequest {
    fn new(operation: ActionCodeOperation) -> Self {
        Self {
            request_type: operation.as_request_type().to_string(),
            email: None,
            new_email: None,
            id_token: None,
            continue_url: None,
            ios_bundle_id: None,
            ios_app_store_id: None,
            android_package_name: None,
            android_install_app: None,
            android_minimum_version_code: None,
            can_handle_code_in_app: None,
            dynamic_link_domain: None,
            link_domain: None,
            captcha_resp: None,
            client_type: None,
            recaptcha_version: None,
            tenant_id: None,
            target_project_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ResetPasswordRequest {
    #[serde(rename = "oobCode")]
    oob_code: String,
    #[serde(rename = "newPassword", skip_serializing_if = "Option::is_none")]
    new_password: Option<String>,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    tenant_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ResetPasswordResponse {
    pub email: Option<String>,
    #[serde(rename = "newEmail")]
    pub new_email: Option<String>,
    #[serde(rename = "requestType")]
    pub request_type: Option<String>,
    #[serde(rename = "mfaInfo")]
    pub mfa_info: Option<Vec<MfaEnrollmentInfo>>,
}

#[derive(Debug, Clone)]
pub enum UpdateString {
    Set(String),
    Clear,
}

#[derive(Debug, Clone)]
pub struct UpdateAccountRequest {
    pub id_token: String,
    pub email: Option<String>,
    pub password: Option<String>,
    pub display_name: Option<UpdateString>,
    pub photo_url: Option<UpdateString>,
    pub delete_providers: Vec<String>,
}

impl UpdateAccountRequest {
    pub fn new(id_token: impl Into<String>) -> Self {
        Self {
            id_token: id_token.into(),
            email: None,
            password: None,
            display_name: None,
            photo_url: None,
            delete_providers: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct UpdateAccountRequestBody {
    #[serde(rename = "idToken")]
    id_token: String,
    #[serde(rename = "email", skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(rename = "password", skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(rename = "photoUrl", skip_serializing_if = "Option::is_none")]
    photo_url: Option<String>,
    #[serde(rename = "deleteAttribute", skip_serializing_if = "Vec::is_empty")]
    delete_attribute: Vec<&'static str>,
    #[serde(rename = "deleteProvider", skip_serializing_if = "Vec::is_empty")]
    delete_provider: Vec<String>,
    #[serde(rename = "returnSecureToken")]
    return_secure_token: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccountResponse {
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expiresIn")]
    pub expires_in: Option<String>,
    #[serde(rename = "localId")]
    pub local_id: Option<String>,
    #[serde(rename = "email")]
    pub email: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "photoUrl")]
    pub photo_url: Option<String>,
    #[serde(rename = "providerUserInfo")]
    pub provider_user_info: Option<Vec<ProviderUserInfo>>,
    #[serde(rename = "mfaInfo")]
    pub mfa_info: Option<Vec<MfaEnrollmentInfo>>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

pub async fn send_password_reset_email(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    email: &str,
) -> AuthResult<()> {
    let mut request = SendOobCodeRequest::new(ActionCodeOperation::PasswordReset);
    request.email = Some(email.to_owned());
    send_oob_code_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        request,
    )
    .await
}

pub async fn send_email_verification(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    id_token: &str,
) -> AuthResult<()> {
    let mut request = SendOobCodeRequest::new(ActionCodeOperation::VerifyEmail);
    request.id_token = Some(id_token.to_owned());
    send_oob_code_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        request,
    )
    .await
}

async fn send_oob_code_async(
    client: Client,
    endpoint: String,
    api_key: String,
    request: SendOobCodeRequest,
) -> AuthResult<()> {
    let url = identity_toolkit_url(&endpoint, "accounts:sendOobCode", &api_key);
    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

pub async fn send_sign_in_link_to_email(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    email: &str,
    settings: &ActionCodeSettings,
) -> AuthResult<()> {
    let mut request = SendOobCodeRequest::new(ActionCodeOperation::EmailSignIn);
    request.email = Some(email.to_owned());
    request.can_handle_code_in_app = Some(settings.handle_code_in_app);
    request.client_type = Some("CLIENT_TYPE_WEB".to_string());
    if !settings.handle_code_in_app {
        return Err(AuthError::InvalidCredential(
            "ActionCodeSettings.handle_code_in_app must be true".into(),
        ));
    }
    apply_action_code_settings(&mut request, settings)?;
    send_oob_code_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        request,
    )
    .await
}

fn apply_action_code_settings(
    request: &mut SendOobCodeRequest,
    settings: &ActionCodeSettings,
) -> AuthResult<()> {
    if settings.url.trim().is_empty() {
        return Err(AuthError::InvalidCredential(
            "ActionCodeSettings.url must not be empty".into(),
        ));
    }
    if let Some(domain) = settings.dynamic_link_domain.as_ref() {
        if domain.trim().is_empty() {
            return Err(AuthError::InvalidCredential(
                "ActionCodeSettings.dynamic_link_domain must not be empty".into(),
            ));
        }
    }
    if let Some(domain) = settings.link_domain.as_ref() {
        if domain.trim().is_empty() {
            return Err(AuthError::InvalidCredential(
                "ActionCodeSettings.link_domain must not be empty".into(),
            ));
        }
    }

    request.continue_url = Some(settings.url.clone());
    request.dynamic_link_domain = settings.dynamic_link_domain.clone();
    request.link_domain = settings.link_domain.clone();
    request.can_handle_code_in_app = Some(settings.handle_code_in_app);

    if let Some(ios) = &settings.i_os {
        if ios.bundle_id.trim().is_empty() {
            return Err(AuthError::InvalidCredential(
                "ActionCodeSettings.i_os.bundle_id must not be empty".into(),
            ));
        }
        request.ios_bundle_id = Some(ios.bundle_id.clone());
    }

    if let Some(android) = &settings.android {
        if android.package_name.trim().is_empty() {
            return Err(AuthError::InvalidCredential(
                "ActionCodeSettings.android.package_name must not be empty".into(),
            ));
        }
        request.android_package_name = Some(android.package_name.clone());
        request.android_install_app = android.install_app;
        request.android_minimum_version_code = android.minimum_version.clone();
    }

    Ok(())
}

pub async fn confirm_password_reset(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    oob_code: &str,
    new_password: &str,
) -> AuthResult<()> {
    reset_password_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        ResetPasswordRequest {
            oob_code: oob_code.to_owned(),
            new_password: Some(new_password.to_owned()),
            tenant_id: None,
        },
    )
    .await
    .map(|_| ())
}

pub async fn reset_password_info(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    oob_code: &str,
    tenant_id: Option<&str>,
) -> AuthResult<ResetPasswordResponse> {
    reset_password_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        ResetPasswordRequest {
            oob_code: oob_code.to_owned(),
            new_password: None,
            tenant_id: tenant_id.map(|t| t.to_owned()),
        },
    )
    .await
}

async fn reset_password_async(
    client: Client,
    endpoint: String,
    api_key: String,
    request: ResetPasswordRequest,
) -> AuthResult<ResetPasswordResponse> {
    let url = identity_toolkit_url(&endpoint, "accounts:resetPassword", &api_key);
    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        let body = response
            .text()
            .await
            .map_err(|err| AuthError::Network(err.to_string()))?;
        if body.trim().is_empty() {
            Ok(ResetPasswordResponse::default())
        } else {
            serde_json::from_str(&body).map_err(|err| AuthError::InvalidCredential(err.to_string()))
        }
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

pub async fn apply_action_code(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    oob_code: &str,
    tenant_id: Option<&str>,
) -> AuthResult<()> {
    let url = identity_toolkit_url(&endpoint, "accounts:update", &api_key);
    let request = ApplyActionCodeRequest {
        oob_code: oob_code.to_owned(),
        tenant_id: tenant_id.map(|t| t.to_owned()),
    };

    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

#[derive(Debug, Serialize)]
struct ApplyActionCodeRequest {
    #[serde(rename = "oobCode")]
    oob_code: String,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    tenant_id: Option<String>,
}

pub async fn update_account(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    params: &UpdateAccountRequest,
) -> AuthResult<UpdateAccountResponse> {
    update_account_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        params.clone(),
    )
    .await
}

async fn update_account_async(
    client: Client,
    endpoint: String,
    api_key: String,
    params: UpdateAccountRequest,
) -> AuthResult<UpdateAccountResponse> {
    let UpdateAccountRequest {
        id_token,
        email,
        password,
        display_name,
        photo_url,
        delete_providers,
    } = params;

    let mut delete_attribute = Vec::new();
    let display_name = match display_name {
        Some(UpdateString::Set(value)) => Some(value),
        Some(UpdateString::Clear) => {
            delete_attribute.push("DISPLAY_NAME");
            None
        }
        None => None,
    };

    let photo_url = match photo_url {
        Some(UpdateString::Set(value)) => Some(value),
        Some(UpdateString::Clear) => {
            delete_attribute.push("PHOTO_URL");
            None
        }
        None => None,
    };

    let request = UpdateAccountRequestBody {
        id_token,
        email,
        password,
        display_name,
        photo_url,
        delete_attribute,
        delete_provider: delete_providers,
        return_secure_token: true,
    };

    let url = identity_toolkit_url(&endpoint, "accounts:update", &api_key);
    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<UpdateAccountResponse>()
            .await
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

pub async fn verify_password(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    request: &SignInWithPasswordRequest,
) -> AuthResult<SignInWithPasswordResponse> {
    verify_password_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        request.clone(),
    )
    .await
}

async fn verify_password_async(
    client: Client,
    endpoint: String,
    api_key: String,
    request: SignInWithPasswordRequest,
) -> AuthResult<SignInWithPasswordResponse> {
    let url = identity_toolkit_url(&endpoint, "accounts:signInWithPassword", &api_key);
    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<SignInWithPasswordResponse>()
            .await
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

pub async fn delete_account(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    id_token: &str,
) -> AuthResult<()> {
    delete_account_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        id_token.to_owned(),
    )
    .await
}

async fn delete_account_async(
    client: Client,
    endpoint: String,
    api_key: String,
    id_token: String,
) -> AuthResult<()> {
    let url = identity_toolkit_url(&endpoint, "accounts:delete", &api_key);
    let request = DeleteAccountRequest { id_token };

    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

#[derive(Debug, Serialize)]
struct DeleteAccountRequest {
    #[serde(rename = "idToken")]
    id_token: String,
}

#[derive(Debug, Serialize)]
struct GetAccountInfoRequest {
    #[serde(rename = "idToken")]
    id_token: String,
}

pub async fn get_account_info(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    id_token: &str,
) -> AuthResult<GetAccountInfoResponse> {
    get_account_info_async(
        client.clone(),
        endpoint.to_owned(),
        api_key.to_owned(),
        id_token.to_owned(),
    )
    .await
}

async fn get_account_info_async(
    client: Client,
    endpoint: String,
    api_key: String,
    id_token: String,
) -> AuthResult<GetAccountInfoResponse> {
    let url = identity_toolkit_url(&endpoint, "accounts:lookup", &api_key);
    let request = GetAccountInfoRequest { id_token };

    let response = client
        .post(url)
        .json(&request)
        .send()
        .await
        .map_err(|err| AuthError::Network(err.to_string()))?;

    if response.status().is_success() {
        response
            .json::<GetAccountInfoResponse>()
            .await
            .map_err(|err| AuthError::InvalidCredential(err.to_string()))
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| String::new());
        Err(map_error(status, body))
    }
}

fn map_error(status: StatusCode, body: String) -> AuthError {
    if let Ok(parsed) = serde_json::from_str::<ErrorResponse>(&body) {
        if let Some(error) = parsed.error {
            if let Some(message) = error.message {
                return AuthError::InvalidCredential(message);
            }
        }
    }

    AuthError::InvalidCredential(format!("Request failed with status {status}: {body}"))
}
