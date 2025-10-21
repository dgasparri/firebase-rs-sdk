use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use percent_encoding::{percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};

use crate::installations::error::{internal_error, InstallationsResult};
use crate::installations::types::InstallationToken;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedAuthToken {
    token: String,
    expires_at_ms: u64,
}

impl PersistedAuthToken {
    pub fn from_runtime(token: &InstallationToken) -> InstallationsResult<Self> {
        let millis = system_time_to_millis(token.expires_at)?;
        Ok(Self {
            token: token.token.clone(),
            expires_at_ms: millis,
        })
    }

    pub fn into_runtime(self) -> InstallationToken {
        InstallationToken {
            token: self.token,
            expires_at: UNIX_EPOCH + Duration::from_millis(self.expires_at_ms),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedInstallation {
    pub fid: String,
    pub refresh_token: String,
    pub auth_token: PersistedAuthToken,
}

pub trait InstallationsPersistence: Send + Sync {
    fn read(&self, app_name: &str) -> InstallationsResult<Option<PersistedInstallation>>;
    fn write(&self, app_name: &str, entry: &PersistedInstallation) -> InstallationsResult<()>;
    fn clear(&self, app_name: &str) -> InstallationsResult<()>;
}

#[derive(Clone, Debug)]
pub struct FilePersistence {
    base_dir: Arc<PathBuf>,
}

impl FilePersistence {
    pub fn new(base_dir: PathBuf) -> InstallationsResult<Self> {
        fs::create_dir_all(&base_dir).map_err(|err| {
            internal_error(format!(
                "Failed to create installations cache directory '{}': {}",
                base_dir.display(),
                err
            ))
        })?;
        Ok(Self {
            base_dir: Arc::new(base_dir),
        })
    }

    pub fn default() -> InstallationsResult<Self> {
        if let Ok(dir) = std::env::var("FIREBASE_INSTALLATIONS_CACHE_DIR") {
            return Self::new(PathBuf::from(dir));
        }

        let dir = std::env::current_dir()
            .map_err(|err| internal_error(format!("Failed to obtain working directory: {}", err)))?
            .join(".firebase/installations");
        Self::new(dir)
    }

    fn file_for(&self, app_name: &str) -> PathBuf {
        let encoded = percent_encode(app_name.as_bytes(), NON_ALPHANUMERIC).to_string();
        self.base_dir.join(format!("{}.json", encoded))
    }
}

impl InstallationsPersistence for FilePersistence {
    fn read(&self, app_name: &str) -> InstallationsResult<Option<PersistedInstallation>> {
        let path = self.file_for(app_name);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|err| {
            internal_error(format!(
                "Failed to read installations cache '{}': {}",
                path.display(),
                err
            ))
        })?;
        let entry = serde_json::from_slice(&bytes).map_err(|err| {
            internal_error(format!(
                "Failed to parse installations cache '{}': {}",
                path.display(),
                err
            ))
        })?;
        Ok(Some(entry))
    }

    fn write(&self, app_name: &str, entry: &PersistedInstallation) -> InstallationsResult<()> {
        let path = self.file_for(app_name);
        let bytes = serde_json::to_vec(entry).map_err(|err| {
            internal_error(format!(
                "Failed to serialize installations cache '{}': {}",
                path.display(),
                err
            ))
        })?;
        fs::write(&path, bytes).map_err(|err| {
            internal_error(format!(
                "Failed to write installations cache '{}': {}",
                path.display(),
                err
            ))
        })
    }

    fn clear(&self, app_name: &str) -> InstallationsResult<()> {
        let path = self.file_for(app_name);
        if path.exists() {
            fs::remove_file(&path).map_err(|err| {
                internal_error(format!(
                    "Failed to delete installations cache '{}': {}",
                    path.display(),
                    err
                ))
            })?;
        }
        Ok(())
    }
}

fn system_time_to_millis(time: SystemTime) -> InstallationsResult<u64> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| internal_error("Token expiration must be after UNIX epoch"))?;
    Ok(duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installations::types::InstallationToken;
    use std::fs;
    use std::time::{Duration, SystemTime};

    fn temp_dir() -> PathBuf {
        let mut path = std::env::temp_dir();
        let unique = format!("installations-persistence-{}", uuid());
        path.push(unique);
        path
    }

    fn uuid() -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        format!("{}", COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    #[test]
    fn file_persistence_round_trip() {
        let dir = temp_dir();
        let persistence = FilePersistence::new(dir.clone()).unwrap();
        let token = InstallationToken {
            token: "token".into(),
            expires_at: SystemTime::now() + Duration::from_secs(60),
        };
        let entry = PersistedInstallation {
            fid: "fid".into(),
            refresh_token: "refresh".into(),
            auth_token: PersistedAuthToken::from_runtime(&token).unwrap(),
        };

        persistence.write("app", &entry).unwrap();
        let loaded = persistence.read("app").unwrap().unwrap();
        assert_eq!(loaded, entry);

        persistence.clear("app").unwrap();
        assert!(persistence.read("app").unwrap().is_none());
        fs::remove_dir_all(dir).ok();
    }
}
