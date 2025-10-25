# Firebase Remote Config Module

This module is the Rust port of the Firebase Remote Config SDK. It exposes a configurable cache of key/value pairs that
can be fetched from the Remote Config backend and activated inside a Firebase app. The current implementation offers an
in-memory stub that wires the component into the shared container so other services can depend on it.

## Porting status

- remote_config 25% `[###       ]`

==As of October 20th, 2025==

Prompt: Compare the original JS/Typescript files in ./packages/remote_config and the ported files in Rust in ./src/remote_config, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

I'm estimating the porting progress at about 25% based on the features done so far—like defaults, fetch stub, storage caching, and typed APIs—but noting key parts still missing such as real HTTP wiring, throttling, custom signals, logging, and real-time updates. This helps ground expectations on remaining work.

I’d put the Remote Config port around 25 % complete. We now mirror the JS SDK’s typed getters, settings surface, fetch metadata, template version tracking, pluggable storage (memory + file), and even have an HTTP client ready once Installations lands. But big pieces remain: wiring that transport by default, throttle/error parity, custom signals, logging controls, real persistent storage for web/wasm, and realtime updates. Once the Installations module is in place and those integrations are finished, the percentage will jump significantly.

## Quick Start Example

```rust,no_run
use std::collections::HashMap;

use firebase_rs_sdk::app::api::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::remote_config::get_remote_config;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        project_id: Some("demo-project".into()),
        ..Default::default()
    };
    let settings = FirebaseAppSettings {
        name: Some("demo-app".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(settings)).await?;
    let remote_config = get_remote_config(Some(app.clone())).await?;

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
- In-memory storage cache that records last fetch status, timestamp, and active config ETAG for parity with
  `StorageCache` in the JS SDK.
- Fetch logic honours `minimum_fetch_interval_millis`, records metadata, and exposes a pluggable
  `RemoteConfigFetchClient` with a blocking HTTP implementation.
- Template version tracking via `active_template_version` to mirror template metadata exposed in the JS SDK.
- Basic string retrieval through `get_string`.
- Simple process-level cache keyed by app name to avoid redundant component creation.
- Minimal error type covering `invalid-argument` and `internal` cases.
- Smoke tests for activation behaviour.

### WASM Notes
- The module compiles for wasm targets when the `wasm-web` feature is enabled. The default fetch client remains a no-op placeholder; a real fetch transport will land once the HTTP wiring is ported.
- Persistent storage still relies on the in-memory cache; future work will add IndexedDB-backed storage under `experimental-indexed-db` similar to the Installations module.

## Still to do

- Persistent storage (web/mobile): add IndexedDB/wasm implementations and select sensible defaults per platform,
  building on the new pluggable storage layer (`packages/remote-config/src/storage`).
- Fetch & transport: implement HTTP fetch logic, throttling, and ETag handling similar to
  `packages/remote-config/src/api.ts` and `client/remote_config_fetch_client.ts`.
- Logging & errors: extend error surface (`ErrorCode` equivalents) and log-level tuning (`setLogLevel`).
- Custom signals & realtime updates: support `setCustomSignals` and realtime update subscriptions (`client/realtime_handler.ts`).
- Testing parity: port fetch/activation/storage tests from `packages/remote-config/test` once functionality exists.

## Next Steps - Detailed Completion Plan

1. **Wire HTTP transport by default (blocked on Installations)**
   - Once the Installations module is ported, supply an `InstallationsProvider` that hands `HttpRemoteConfigFetchClient` the installation ID/token.
   - Extend fetch handling with throttle metadata persistence and richer error mapping mirroring JS error codes.
2. **Add platform-specific persistent storage**
   - Provide IndexedDB/wasm implementations and choose defaults per target while keeping the file backend for native.
   - Mirror JS quota/cleanup behaviour and test warm-up flows across restarts.
3. **Integrate custom signals and logging controls**
   - Expose setter for custom signals stored alongside metadata; allow toggling between error/debug/silent log levels.
   - Provide tests ensuring signals persist and log level changes propagate.

Work paused here until the Installations module is available; revisit step 1 afterwards to hook up real network fetches.
