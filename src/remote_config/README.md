# Firebase Remote Config Port (Rust)

## Introduction

This module is the Rust port of the Firebase Remote Config SDK. It exposes a configurable cache of key/value pairs that
can be fetched from the Remote Config backend and activated inside a Firebase app. The current implementation offers an
in-memory stub that wires the component into the shared container so other services can depend on it.

## Porting status

- remote_config 3% \[          \]

==As of October 20th, 2025==

Prompt: Compare the original JS/Typescript files in ./packages/remote_config and the ported files in Rust in ./src/remote_config, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

Remote Config coverage sits at roughly 3 %.

  - Rust mirrors only the scaffolding: a component registration plus an in-memory RemoteConfig that stores defaults and
    copies them into active values on activate, while fetch is a no-op (src/remote_config/api.rs).
  - The JS SDK is far richer, with initialization options, storage caching, network fetch via RemoteConfigFetchClient,
    realtime handlers, settings tweaks, typed getters (getBoolean, getNumber, etc.), custom signals, logging, fetch
    throttling, minimum intervals, persistence with Storage/IndexedDB, and realtime updates (packages/remote-config/src/
    api.ts, packages/remote-config/src/remote_config.ts, and related client, storage, value, errors, register code).

## Quick Start Example

```rust
use std::collections::HashMap;

use firebase_rs_sdk_unofficial::app::api::initialize_app;
use firebase_rs_sdk_unofficial::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk_unofficial::remote_config::{get_remote_config, RemoteConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        project_id: Some("demo-project".into()),
        ..Default::default()
    };
    let settings = FirebaseAppSettings {
        name: Some("demo-app".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(settings))?;
    let remote_config = get_remote_config(Some(app.clone()))?;

    remote_config.set_defaults(HashMap::from([(String::from("welcome"), String::from("hello"))]));
    remote_config.fetch()?;
    remote_config.activate()?;

    println!("welcome = {}", remote_config.get_string("welcome"));
    Ok(())
}
```

## Implemented

- Component registration for `remote-config`, allowing `get_remote_config` to resolve instances through the shared
  component container (src/remote_config/api.rs, packages/remote-config/src/register.ts).
- In-memory default store with `set_defaults`, `fetch`, and `activate` copying defaults into the active map once.
- Value API parity with typed getters (`get_value`, `get_boolean`, `get_number`, `get_all`) backed by
  `RemoteConfigValue`.
- Config settings surface with validation, including fetch timeout and minimum fetch interval knobs analogous to the JS SDK.
- Basic string retrieval through `get_string`.
- Simple process-level cache keyed by app name to avoid redundant component creation.
- Minimal error type covering `invalid-argument` and `internal` cases.
- Smoke tests for activation behaviour.

## Still to do

- Fetch & transport: implement HTTP fetch logic, throttling, and ETag handling similar to
  `packages/remote-config/src/api.ts` and `client/remote_config_fetch_client.ts`.
- Persistent storage: port `Storage` + `StorageCache` for active configs, last fetch metadata, and cross-process
  durability (`packages/remote-config/src/storage`).
- Logging & errors: extend error surface (`ErrorCode` equivalents) and log-level tuning (`setLogLevel`).
- Custom signals & realtime updates: support `setCustomSignals` and realtime update subscriptions (`client/realtime_handler.ts`).
- Testing parity: port fetch/activation/storage tests from `packages/remote-config/test` once functionality exists.

## Next Steps - Detailed Completion Plan

1. **Implement fetch metadata persistence**
   - Introduce a storage trait with in-memory implementation for now, emulating `StorageCache` behaviour.
   - Track last fetch status, timestamp, and active config ETAG, paving the path for real network fetches.
2. **Implement fetch/transport logic**
   - Sketch a fetch client abstraction that can be backed by reqwest (native) or web-sys (wasm).
   - Honor settings for timeout and minimum fetch interval, and surface structured errors.
3. **Integrate custom signals and logging controls**
   - Expose setter for custom signals stored alongside metadata; allow toggling between error/debug/silent log levels.
   - Provide tests ensuring signals persist and log level changes propagate.

Completing step 1 exposes configuration knobs that other crates rely on, setting the stage for real fetch logic and persistence work.
