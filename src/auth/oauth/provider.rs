use std::collections::HashMap;

use url::Url;


use crate::auth::api::Auth;
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::model::UserCredential;
use crate::auth::oauth::RedirectOperation;
use super::OAuthRequest;

/// Builder-like representation of an OAuth identity provider.
///
/// The provider stores configuration (scopes, custom parameters, language hints)
/// and creates [`OAuthRequest`] instances that can be routed through the popup
/// or redirect handlers registered on [`Auth`].
#[derive(Debug, Clone)]
pub struct OAuthProvider {
    provider_id: String,
    authorization_endpoint: String,
    scopes: Vec<String>,
    custom_parameters: HashMap<String, String>,
    display_name: Option<String>,
    language_code: Option<String>,
}

impl OAuthProvider {
    pub fn new(provider_id: impl Into<String>, authorization_endpoint: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            authorization_endpoint: authorization_endpoint.into(),
            scopes: Vec::new(),
            custom_parameters: HashMap::new(),
            display_name: None,
            language_code: None,
        }
    }

    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub fn authorization_endpoint(&self) -> &str {
        &self.authorization_endpoint
    }

    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    pub fn custom_parameters(&self) -> &HashMap<String, String> {
        &self.custom_parameters
    }

    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    pub fn language_code(&self) -> Option<&str> {
        self.language_code.as_deref()
    }

    pub fn add_scope(&mut self, scope: impl Into<String>) {
        let value = scope.into();
        if !self.scopes.contains(&value) {
            self.scopes.push(value);
        }
    }

    pub fn set_scopes<I, S>(&mut self, scopes: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scopes.clear();
        self.scopes.extend(scopes.into_iter().map(Into::into));
    }

    pub fn set_custom_parameters(&mut self, parameters: HashMap<String, String>) -> &mut Self {
        self.custom_parameters = parameters;
        self
    }

    pub fn set_display_name(&mut self, value: impl Into<String>) -> &mut Self {
        self.display_name = Some(value.into());
        self
    }

    pub fn set_language_code(&mut self, value: impl Into<String>) -> &mut Self {
        self.language_code = Some(value.into());
        self
    }

    /// Builds the `OAuthRequest` that will be passed to popup/redirect handlers.
    pub fn build_request(&self, auth: &Auth) -> AuthResult<OAuthRequest> {
        let mut url = Url::parse(&self.authorization_endpoint).map_err(|err| {
            AuthError::InvalidCredential(format!(
                "Invalid authorization endpoint for provider {}: {err}",
                self.provider_id
            ))
        })?;

        {
            let mut pairs = url.query_pairs_mut();
            if !self.scopes.is_empty() {
                pairs.append_pair("scope", &self.scopes.join(" "));
            }
            if let Some(lang) = &self.language_code {
                pairs.append_pair("hl", lang);
            }
            if let Some(auth_domain) = auth.app().options().auth_domain {
                pairs.append_pair("auth_domain", &auth_domain);
            }
            if let Some(api_key) = auth.app().options().api_key {
                pairs.append_pair("apiKey", &api_key);
            }
            for (key, value) in &self.custom_parameters {
                pairs.append_pair(key, value);
            }
        }

        let auth_url: String = url.into();
        let mut request = OAuthRequest::new(self.provider_id.clone(), auth_url);
        if let Some(display) = &self.display_name {
            request = request.with_display_name(display.clone());
        }
        if let Some(lang) = &self.language_code {
            request = request.with_language_code(lang.clone());
        }
        request = request.with_custom_parameters(self.custom_parameters.clone());
        Ok(request)
    }

    /// Runs the configured popup handler and returns the produced credential.
    pub fn sign_in_with_popup(&self, auth: &Auth) -> AuthResult<UserCredential> {
        let handler = auth.popup_handler().ok_or(AuthError::NotImplemented(
            "OAuth popup handler not registered",
        ))?;
        let request = self.build_request(auth)?;
        let credential = handler.open_popup(request)?;
        auth.sign_in_with_oauth_credential(credential)
    }

    pub fn link_with_popup(&self, auth: &Auth) -> AuthResult<UserCredential> {
        let handler = auth.popup_handler().ok_or(AuthError::NotImplemented(
            "OAuth popup handler not registered",
        ))?;
        let request = self.build_request(auth)?;
        let credential = handler.open_popup(request)?;
        auth.link_with_oauth_credential(credential)
    }

    /// Delegates to the redirect handler to start a redirect based flow.
    pub fn sign_in_with_redirect(&self, auth: &Auth) -> AuthResult<()> {
        let handler = auth.redirect_handler().ok_or(AuthError::NotImplemented(
            "OAuth redirect handler not registered",
        ))?;
        auth.set_pending_redirect_event(&self.provider_id, RedirectOperation::SignIn)?;
        let request = self.build_request(auth)?;
        if let Err(err) = handler.initiate_redirect(request) {
            auth.clear_pending_redirect_event()?;
            return Err(err);
        }
        Ok(())
    }

    pub fn link_with_redirect(&self, auth: &Auth) -> AuthResult<()> {
        let handler = auth.redirect_handler().ok_or(AuthError::NotImplemented(
            "OAuth redirect handler not registered",
        ))?;
        auth.set_pending_redirect_event(&self.provider_id, RedirectOperation::Link)?;
        let request = self.build_request(auth)?;
        if let Err(err) = handler.initiate_redirect(request) {
            auth.clear_pending_redirect_event()?;
            return Err(err);
        }
        Ok(())
    }

    /// Completes a redirect flow using the registered redirect handler.
    ///
    /// The provider does not influence result parsing at this stage; the
    /// handler is responsible for decoding whichever callback mechanism the
    /// hosting platform uses.
    pub fn get_redirect_result(auth: &Auth) -> AuthResult<Option<UserCredential>> {
        let handler = auth.redirect_handler().ok_or(AuthError::NotImplemented(
            "OAuth redirect handler not registered",
        ))?;
        let pending = auth.take_pending_redirect_event()?;
        if pending.is_none() {
            return Ok(None);
        }
        let pending = pending.unwrap();

        match handler.complete_redirect()? {
            Some(credential) => match pending.operation {
                RedirectOperation::Link => auth.link_with_oauth_credential(credential).map(Some),
                RedirectOperation::SignIn => {
                    auth.sign_in_with_oauth_credential(credential).map(Some)
                }
            },
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FirebaseApp, FirebaseAppConfig, FirebaseOptions};
    use crate::component::ComponentContainer;

    use crate::auth::api::Auth;

    use std::sync::{Arc, Mutex};

    fn build_test_auth() -> Arc<Auth> {
        let options = FirebaseOptions {
            api_key: Some("test-key".into()),
            auth_domain: Some("example.firebaseapp.com".into()),
            ..Default::default()
        };
        let config = FirebaseAppConfig::new("test-app", false);
        let container = ComponentContainer::new("test-app");
        let app = FirebaseApp::new(options, config, container);
        Auth::builder(app).build().unwrap()
    }

    #[test]
    fn build_request_includes_scopes_and_params() {
        let auth = build_test_auth();
        let mut provider = OAuthProvider::new("google.com", "https://example.com/oauth");
        provider.add_scope("profile");
        provider.set_language_code("en");
        provider.set_custom_parameters(
            [("prompt".to_string(), "select_account".to_string())]
                .into_iter()
                .collect(),
        );

        let request = provider.build_request(&auth).unwrap();
        assert!(request.auth_url.contains("scope=profile"));
        assert!(request.auth_url.contains("apiKey=test-key"));
        assert!(request
            .auth_url
            .contains("auth_domain=example.firebaseapp.com"));
        assert!(request.auth_url.contains("prompt=select_account"));
        assert_eq!(request.provider_id, "google.com");
    }

    struct RecordingRedirectHandler {
        fail: bool,
        initiated: Arc<Mutex<bool>>,
    }

    impl crate::auth::OAuthRedirectHandler for RecordingRedirectHandler {
        fn initiate_redirect(&self, _request: OAuthRequest) -> AuthResult<()> {
            *self.initiated.lock().unwrap() = true;
            if self.fail {
                Err(AuthError::InvalidCredential("failure".into()))
            } else {
                Ok(())
            }
        }

        fn complete_redirect(&self) -> AuthResult<Option<crate::auth::oauth::AuthCredential>> {
            Ok(None)
        }
    }

    #[test]
    fn link_with_redirect_sets_and_clears_event_on_success() {
        let auth = build_test_auth();
        let handler = Arc::new(RecordingRedirectHandler {
            fail: false,
            initiated: Arc::new(Mutex::new(false)),
        });
        auth.set_redirect_handler(handler);

        let provider = OAuthProvider::new("google.com", "https://example.com");
        provider.link_with_redirect(&auth).unwrap();
        let event = auth.take_pending_redirect_event().unwrap();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.provider_id, "google.com");
        assert_eq!(event.operation, RedirectOperation::Link);
    }

    #[test]
    fn link_with_redirect_clears_on_failure() {
        let auth = build_test_auth();
        let handler = Arc::new(RecordingRedirectHandler {
            fail: true,
            initiated: Arc::new(Mutex::new(false)),
        });
        auth.set_redirect_handler(handler);

        let provider = OAuthProvider::new("google.com", "https://example.com");
        assert!(provider.link_with_redirect(&auth).is_err());
        assert!(auth.take_pending_redirect_event().unwrap().is_none());
    }
}
