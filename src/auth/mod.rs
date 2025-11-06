#![doc = include_str!("README.md")]
mod api;
mod error;
mod model;
mod oauth;
mod persistence;
mod phone;
mod token_manager;

//#![cfg(feature = "firestore")]
mod token_provider;
mod types;

#[doc(inline)]
pub use api::{
    auth_for_app, refresh_id_token, refresh_id_token_with_endpoint, register_auth_component, Auth,
    AuthBuilder, RefreshTokenResponse,
};

#[allow(unused_imports)]
pub(crate) use api::DEFAULT_SECURE_TOKEN_ENDPOINT;

#[doc(inline)]
pub use error::{AuthError, AuthResult, MultiFactorAuthError, MultiFactorAuthErrorCode};

#[allow(unused_imports)]
pub(crate) use error::map_mfa_error_code;

#[doc(inline)]
pub use model::{
    AccountInfoUser, AuthConfig, AuthCredential, AuthStateListeners, EmailAuthProvider,
    GetAccountInfoResponse, MfaEnrollmentInfo, ProviderUserInfo, SignInWithCustomTokenRequest,
    SignInWithCustomTokenResponse, SignInWithEmailLinkRequest, SignInWithEmailLinkResponse,
    SignInWithPasswordRequest, SignInWithPasswordResponse, SignUpRequest, SignUpResponse, User,
    UserCredential, UserInfo,
};

#[doc(inline)]
pub use oauth::{
    credential::OAuthCredential,
    pkce::PkcePair,
    provider::OAuthProvider,
    providers::{
        oauth_access_token_map, AppleAuthProvider, FacebookAuthProvider, GitHubAuthProvider,
        GoogleAuthProvider, MicrosoftAuthProvider, OAuthProviderFactory, TwitterAuthProvider,
        YahooAuthProvider,
    },
    redirect::{
        InMemoryRedirectPersistence, PendingRedirectEvent, RedirectOperation, RedirectPersistence,
    },
    OAuthPopupHandler, OAuthRedirectHandler, OAuthRequest,
};

#[doc(inline)]
pub use persistence::{
    AuthPersistence, ClosurePersistence, InMemoryPersistence, PersistedAuthState,
    PersistenceListener, PersistenceSubscription,
};

// persistence::indexed_db::IndexedDbPersistence;
#[cfg(all(
    feature = "wasm-web",
    target_arch = "wasm32",
    feature = "experimental-indexed-db"
))]
#[doc(inline)]
pub use persistence::indexed_db::IndexedDbPersistence;

// persistence::file::FilePersistence;
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]
#[doc(inline)]
pub use persistence::file::FilePersistence;

// persistence::web::{WebStorageDriver, WebStoragePersistence};
#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
#[doc(inline)]
pub use persistence::web::{WebStorageDriver, WebStoragePersistence};

#[doc(inline)]
pub use phone::{
    PhoneAuthCredential, PhoneAuthProvider, PhoneMultiFactorGenerator, PHONE_PROVIDER_ID,
};

//#![cfg(feature = "firestore")]
#[doc(inline)]
pub use token_provider::{auth_token_provider_arc, AuthTokenProvider};

#[doc(inline)]
pub use types::{
    get_multi_factor_resolver, ActionCodeInfo, ActionCodeInfoData, ActionCodeOperation,
    ActionCodeSettings, ActionCodeUrl, AdditionalUserInfo, AndroidSettings, ApplicationVerifier,
    AuthSettings, AuthStateListener, ConfirmationResult, FirebaseAuth, IdTokenResult, IosSettings,
    MultiFactorAssertion, MultiFactorError, MultiFactorInfo, MultiFactorOperation,
    MultiFactorResolver, MultiFactorSession, MultiFactorSessionType, MultiFactorUser, Observer,
    PhoneMultiFactorAssertion, TotpMultiFactorAssertion, TotpMultiFactorGenerator, TotpSecret,
    UserMetadata, WebAuthnAssertionKind, WebAuthnAssertionResponse, WebAuthnAttestationResponse,
    WebAuthnCredentialDescriptor, WebAuthnEnrollmentChallenge, WebAuthnMultiFactorAssertion,
    WebAuthnMultiFactorGenerator, WebAuthnSignInChallenge, WebAuthnTransport, WEBAUTHN_FACTOR_ID,
};

#[allow(unused_imports)]
pub(crate) use types::MultiFactorSignInContext;
