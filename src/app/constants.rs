use std::collections::HashMap;
use std::sync::LazyLock;

pub const DEFAULT_ENTRY_NAME: &str = "[DEFAULT]";

pub static PLATFORM_LOG_STRING: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("@firebase/app", "fire-core"),
        ("@firebase/app-compat", "fire-core-compat"),
        ("@firebase/analytics", "fire-analytics"),
        ("@firebase/analytics-compat", "fire-analytics-compat"),
        ("@firebase/app-check", "fire-app-check"),
        ("@firebase/app-check-compat", "fire-app-check-compat"),
        ("@firebase/auth", "fire-auth"),
        ("@firebase/auth-compat", "fire-auth-compat"),
        ("@firebase/database", "fire-rtdb"),
        ("@firebase/data-connect", "fire-data-connect"),
        ("@firebase/database-compat", "fire-rtdb-compat"),
        ("@firebase/functions", "fire-fn"),
        ("@firebase/functions-compat", "fire-fn-compat"),
        ("@firebase/installations", "fire-iid"),
        ("@firebase/installations-compat", "fire-iid-compat"),
        ("@firebase/messaging", "fire-fcm"),
        ("@firebase/messaging-compat", "fire-fcm-compat"),
        ("@firebase/performance", "fire-perf"),
        ("@firebase/performance-compat", "fire-perf-compat"),
        ("@firebase/remote-config", "fire-rc"),
        ("@firebase/remote-config-compat", "fire-rc-compat"),
        ("@firebase/storage", "fire-gcs"),
        ("@firebase/storage-compat", "fire-gcs-compat"),
        ("@firebase/firestore", "fire-fst"),
        ("@firebase/firestore-compat", "fire-fst-compat"),
        ("@firebase/ai", "fire-vertex"),
        ("fire-js", "fire-js"),
        ("firebase", "fire-js-all"),
    ])
});
