use std::env;

fn forced_environment() -> Option<String> {
    env::var("FIREBASE_FORCE_ENVIRONMENT").ok()
}

pub fn get_user_agent() -> String {
    env::var("FIREBASE_USER_AGENT").unwrap_or_default()
}

pub fn is_mobile_cordova() -> bool {
    false
}

pub fn is_node() -> bool {
    match forced_environment().as_deref() {
        Some("browser") => false,
        Some("node") => true,
        _ => true,
    }
}

pub fn is_browser() -> bool {
    match forced_environment().as_deref() {
        Some("browser") => true,
        Some("node") => false,
        _ => false,
    }
}

pub fn is_web_worker() -> bool {
    false
}

pub fn is_cloudflare_worker() -> bool {
    false
}

pub fn is_browser_extension() -> bool {
    false
}

pub fn is_react_native() -> bool {
    false
}

pub fn is_electron() -> bool {
    get_user_agent().contains("Electron/")
}

pub fn is_safari() -> bool {
    false
}

pub fn is_safari_or_webkit() -> bool {
    false
}

pub fn is_indexed_db_available() -> bool {
    false
}

pub fn are_cookies_enabled() -> bool {
    false
}

pub fn is_uwp() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_environment_is_node() {
        assert!(is_node());
        assert!(!is_browser());
    }

    #[test]
    fn detect_electron_user_agent() {
        unsafe { env::set_var("FIREBASE_USER_AGENT", "Electron/1.0") };
        assert!(is_electron());
        unsafe { env::remove_var("FIREBASE_USER_AGENT") };
    }
}
