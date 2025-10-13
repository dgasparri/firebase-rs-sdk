use std::collections::HashMap;

use super::OAuthProvider;

pub trait OAuthProviderFactory {
    fn provider_id() -> &'static str;
    fn new() -> OAuthProvider;
}

pub struct GoogleAuthProvider;

impl GoogleAuthProvider {
    pub fn add_login_hint(provider: &mut OAuthProvider, hint: &str) {
        let mut params = provider.custom_parameters().clone();
        params.insert("login_hint".to_string(), hint.to_string());
        provider.set_custom_parameters(params);
    }
}

impl OAuthProviderFactory for GoogleAuthProvider {
    fn provider_id() -> &'static str {
        "google.com"
    }

    fn new() -> OAuthProvider {
        let mut provider = OAuthProvider::new(
            Self::provider_id(),
            "https://accounts.google.com/o/oauth2/v2/auth",
        );
        provider.add_scope("profile");
        provider.add_scope("email");
        provider.set_custom_parameters(
            [("prompt".to_string(), "select_account".to_string())]
                .into_iter()
                .collect(),
        );
        provider
    }
}

pub struct FacebookAuthProvider;

impl OAuthProviderFactory for FacebookAuthProvider {
    fn provider_id() -> &'static str {
        "facebook.com"
    }

    fn new() -> OAuthProvider {
        let mut provider = OAuthProvider::new(
            Self::provider_id(),
            "https://www.facebook.com/v12.0/dialog/oauth",
        );
        provider.add_scope("email");
        provider
    }
}

pub struct GithubAuthProvider;

impl GithubAuthProvider {
    pub fn set_login_hint(provider: &mut OAuthProvider, login: &str) {
        let mut params = provider.custom_parameters().clone();
        params.insert("login".to_string(), login.to_string());
        provider.set_custom_parameters(params);
    }
}

impl OAuthProviderFactory for GithubAuthProvider {
    fn provider_id() -> &'static str {
        "github.com"
    }

    fn new() -> OAuthProvider {
        let mut provider = OAuthProvider::new(
            Self::provider_id(),
            "https://github.com/login/oauth/authorize",
        );
        provider.add_scope("read:user");
        provider
    }
}

pub struct TwitterAuthProvider;

impl OAuthProviderFactory for TwitterAuthProvider {
    fn provider_id() -> &'static str {
        "twitter.com"
    }

    fn new() -> OAuthProvider {
        let mut provider = OAuthProvider::new(
            Self::provider_id(),
            "https://twitter.com/i/oauth2/authorize",
        );
        provider.add_scope("tweet.read");
        provider.add_scope("users.read");
        provider
    }
}

pub struct MicrosoftAuthProvider;

impl OAuthProviderFactory for MicrosoftAuthProvider {
    fn provider_id() -> &'static str {
        "microsoft.com"
    }

    fn new() -> OAuthProvider {
        let mut provider = OAuthProvider::new(
            Self::provider_id(),
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        );
        provider.add_scope("openid");
        provider.add_scope("profile");
        provider.add_scope("email");
        provider
    }
}

pub fn oauth_access_token_map(token: &str) -> HashMap<String, String> {
    [("oauthAccessToken".to_string(), token.to_string())]
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_defaults_include_prompt() {
        let provider = GoogleAuthProvider::new();
        assert_eq!(provider.provider_id(), "google.com");
        assert!(provider
            .custom_parameters()
            .get("prompt")
            .map(|value| value == "select_account")
            .unwrap_or(false));
    }

    #[test]
    fn github_login_hint() {
        let mut provider = GithubAuthProvider::new();
        GithubAuthProvider::set_login_hint(&mut provider, "octocat");
        assert_eq!(
            provider.custom_parameters().get("login"),
            Some(&"octocat".to_string())
        );
    }

    #[test]
    fn oauth_access_token_helper() {
        let map = oauth_access_token_map("token");
        assert_eq!(map.get("oauthAccessToken"), Some(&"token".to_string()));
    }
}
