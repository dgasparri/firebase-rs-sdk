use std::cmp::Ordering;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Default)]
struct TokenState {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expiration_time: Option<SystemTime>,
}

#[derive(Debug, Default)]
pub struct TokenManager {
    state: Mutex<TokenState>,
}

impl Clone for TokenManager {
    fn clone(&self) -> Self {
        let state = self.state.lock().unwrap().clone();
        Self {
            state: Mutex::new(state),
        }
    }
}

impl TokenManager {
    pub fn update(&self, update: TokenUpdate) {
        let mut state = self.state.lock().unwrap();
        if let Some(access_token) = update.access_token {
            state.access_token = Some(access_token);
        }
        if let Some(refresh_token) = update.refresh_token {
            state.refresh_token = Some(refresh_token);
        }
        if let Some(expires_in) = update.expires_in {
            state.expiration_time = SystemTime::now().checked_add(expires_in);
        }
    }

    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        *state = TokenState::default();
    }

    pub fn access_token(&self) -> Option<String> {
        self.state.lock().unwrap().access_token.clone()
    }

    pub fn refresh_token(&self) -> Option<String> {
        self.state.lock().unwrap().refresh_token.clone()
    }

    pub fn should_refresh(&self, tolerance: Duration) -> bool {
        let state = self.state.lock().unwrap();
        if state.access_token.is_none() {
            return true;
        }
        match state.expiration_time {
            None => false,
            Some(expiration) => {
                let now = SystemTime::now();
                let threshold = now.checked_add(tolerance).unwrap_or_else(SystemTime::now);
                matches!(expiration.cmp(&threshold), Ordering::Less | Ordering::Equal)
            }
        }
    }

    pub fn expiration_time(&self) -> Option<SystemTime> {
        self.state.lock().unwrap().expiration_time
    }

    pub fn initialize(
        &self,
        access_token: Option<String>,
        refresh_token: Option<String>,
        expiration_time: Option<SystemTime>,
    ) {
        let mut state = self.state.lock().unwrap();
        state.access_token = access_token;
        state.refresh_token = refresh_token;
        state.expiration_time = expiration_time;
    }
}

#[derive(Debug)]
pub struct TokenUpdate {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: Option<Duration>,
}

impl TokenUpdate {
    pub fn new(
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_in: Option<Duration>,
    ) -> Self {
        Self {
            access_token,
            refresh_token,
            expires_in,
        }
    }
}
