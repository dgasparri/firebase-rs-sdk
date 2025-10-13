use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;

use crate::app::errors::AppResult;
use crate::app::platform_logger::PlatformLoggerServiceImpl;
use crate::app::types::{
    FirebaseApp, HeartbeatService, HeartbeatStorage, HeartbeatsInStorage, PlatformLoggerService,
    SingleDateHeartbeat,
};
use crate::component::ComponentContainer;

const MAX_NUM_STORED_HEARTBEATS: usize = 30;
#[allow(dead_code)]
const MAX_HEADER_BYTES: usize = 1024;

pub struct HeartbeatServiceImpl {
    app: FirebaseApp,
    storage: Arc<dyn HeartbeatStorage>,
    cache: Mutex<Option<HeartbeatsInStorage>>,
}

impl HeartbeatServiceImpl {
    pub fn new(app: FirebaseApp, storage: Arc<dyn HeartbeatStorage>) -> Self {
        Self {
            app,
            storage,
            cache: Mutex::new(None),
        }
    }

    fn load_cache(&self) -> AppResult<HeartbeatsInStorage> {
        let mut guard = self.cache.lock().unwrap();
        if let Some(cache) = guard.clone() {
            return Ok(cache);
        }
        let cache = self.storage.read()?;
        *guard = Some(cache.clone());
        Ok(cache)
    }

    fn update_cache(&self, value: HeartbeatsInStorage) {
        let mut guard = self.cache.lock().unwrap();
        *guard = Some(value);
    }

    fn platform_agent(container: &ComponentContainer) -> Option<String> {
        container
            .get_provider("platform-logger")
            .get_immediate::<PlatformLoggerServiceImpl>()
            .map(|service| service.platform_info_string())
            .filter(|s| !s.is_empty())
    }

    fn today_utc() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }

    fn prune_oldest(heartbeats: &mut Vec<SingleDateHeartbeat>) {
        if heartbeats.len() <= MAX_NUM_STORED_HEARTBEATS {
            return;
        }
        if let Some((index, _)) = heartbeats
            .iter()
            .enumerate()
            .min_by_key(|(_, hb)| hb.date.clone())
        {
            heartbeats.remove(index);
        }
    }

    #[allow(dead_code)]
    fn header_payload(heartbeats: &[SingleDateHeartbeat]) -> HeartbeatHeaderResult {
        let mut selected: Vec<HeartbeatsByUserAgent> = Vec::new();
        let mut unsent = Vec::new();

        for hb in heartbeats.iter().cloned() {
            let entry = selected
                .iter_mut()
                .find(|existing| existing.agent == hb.agent);
            if let Some(existing) = entry {
                existing.dates.push(hb.date.clone());
            } else {
                selected.push(HeartbeatsByUserAgent {
                    agent: hb.agent.clone(),
                    dates: vec![hb.date.clone()],
                });
            }

            if let Some(encoded) = encode_entries(&selected) {
                if encoded.len() <= MAX_HEADER_BYTES {
                    continue;
                }
            }

            if let Some(existing) = selected
                .iter_mut()
                .find(|existing| existing.agent == hb.agent)
            {
                existing.dates.pop();
                if existing.dates.is_empty() {
                    selected.retain(|entry| entry.agent != hb.agent);
                }
            }
            unsent.push(hb);
        }

        HeartbeatHeaderResult {
            heartbeats_to_send: selected,
            unsent,
        }
    }
}

impl HeartbeatService for HeartbeatServiceImpl {
    fn trigger_heartbeat(&self) -> AppResult<()> {
        let mut cache = self.load_cache()?;
        let date = Self::today_utc();

        if cache.last_sent_heartbeat_date.as_deref() == Some(&date) {
            return Ok(());
        }

        if cache
            .heartbeats
            .iter()
            .any(|heartbeat| heartbeat.date == date)
        {
            return Ok(());
        }

        let agent =
            Self::platform_agent(&self.app.container()).unwrap_or_else(|| "unknown".to_string());
        cache.heartbeats.push(SingleDateHeartbeat { date, agent });
        Self::prune_oldest(&mut cache.heartbeats);
        self.storage.overwrite(&cache)?;
        self.update_cache(cache);
        Ok(())
    }

    fn heartbeats_header(&self) -> AppResult<Option<String>> {
        let mut cache = self.load_cache()?;
        if cache.heartbeats.is_empty() {
            return Ok(None);
        }

        let result = Self::header_payload(&cache.heartbeats);
        if result.heartbeats_to_send.is_empty() {
            return Ok(None);
        }

        let header = encode_entries(&result.heartbeats_to_send).unwrap_or_else(|| "".to_string());
        if header.is_empty() {
            return Ok(None);
        }

        cache.heartbeats = result.unsent;
        cache.last_sent_heartbeat_date = Some(Self::today_utc());
        self.storage.overwrite(&cache)?;
        self.update_cache(cache);

        Ok(Some(header))
    }
}

pub struct InMemoryHeartbeatStorage {
    key: String,
}

impl InMemoryHeartbeatStorage {
    pub fn new(app: &FirebaseApp) -> Self {
        let options = app.options();
        let key = format!("{}!{}", app.name(), options.app_id.unwrap_or_default());
        Self { key }
    }
}

impl HeartbeatStorage for InMemoryHeartbeatStorage {
    fn read(&self) -> AppResult<HeartbeatsInStorage> {
        let store = HEARTBEAT_STORE.lock().unwrap();
        Ok(store
            .get(&self.key)
            .cloned()
            .unwrap_or_else(HeartbeatsInStorage::default))
    }

    fn overwrite(&self, value: &HeartbeatsInStorage) -> AppResult<()> {
        HEARTBEAT_STORE
            .lock()
            .unwrap()
            .insert(self.key.clone(), value.clone());
        Ok(())
    }
}

static HEARTBEAT_STORE: LazyLock<Mutex<HashMap<String, HeartbeatsInStorage>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[allow(dead_code)]
struct HeartbeatsByUserAgent {
    agent: String,
    dates: Vec<String>,
}

#[allow(dead_code)]
fn encode_entries(entries: &[HeartbeatsByUserAgent]) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    #[derive(Serialize)]
    struct HeartbeatEntry<'a> {
        agent: &'a str,
        dates: &'a [String],
    }

    let heartbeats: Vec<HeartbeatEntry<'_>> = entries
        .iter()
        .map(|entry| HeartbeatEntry {
            agent: entry.agent.as_str(),
            dates: &entry.dates,
        })
        .collect();

    let payload = json!({ "version": 2, "heartbeats": heartbeats });
    let serialized = serde_json::to_string(&payload).ok()?;
    Some(URL_SAFE_NO_PAD.encode(serialized))
}

#[allow(dead_code)]
struct HeartbeatHeaderResult {
    heartbeats_to_send: Vec<HeartbeatsByUserAgent>,
    unsent: Vec<SingleDateHeartbeat>,
}
