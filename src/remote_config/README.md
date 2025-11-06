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

use firebase_rs_sdk::app::initialize_app;
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
    if remote_config.fetch_and_activate().await? {
        println!("Activated freshly downloaded parameters");
    }

    println!("welcome = {}", remote_config.get_string("welcome"));
    Ok(())
}
```

## Implemented

- Component registration for `remote-config`, allowing `get_remote_config` to resolve instances through the shared
  component container (src/remote_config/api.rs, packages/remote-config/src/register.ts).
- In-memory default store with `set_defaults`, `fetch`, and async `activate` mirroring the JS lifecycle.
- Value API parity with typed getters (`get_value`, `get_boolean`, `get_number`, `get_all`) backed by
  `RemoteConfigValue`.
- Config settings surface with validation, including fetch timeout and minimum fetch interval knobs analogous to the JS SDK.
- Async storage cache that records last fetch status, timestamp, active config ETAG, template version, and custom signals,
  matching the behaviour of the JS SDK `StorageCache` abstraction.
- IndexedDB-backed persistence for wasm builds (behind `wasm-web` + `experimental-indexed-db`) so values survive reloads and
  custom signals can be shared across tabs.
- Fetch logic honours `minimum_fetch_interval_millis`, records metadata, and exposes a pluggable
  `RemoteConfigFetchClient` with async HTTP implementations for native (`HttpRemoteConfigFetchClient`) and wasm (`WasmRemoteConfigFetchClient`).
- Template version tracking via `active_template_version` to mirror template metadata exposed in the JS SDK.
- Basic string retrieval through `get_string`.
- Simple process-level cache keyed by app name to avoid redundant component creation.
- Minimal error type covering `invalid-argument` and `internal` cases.
- Async helpers `ensure_initialized`, `fetch_and_activate`, and `set_custom_signals` that align with the modular JS API.
- Smoke tests for activation behaviour and custom signal forwarding.

### WASM Notes
- The module compiles for wasm targets when the `wasm-web` feature is enabled. The default fetch client uses
  [`WasmRemoteConfigFetchClient`](crate::remote_config::fetch::WasmRemoteConfigFetchClient) to perform real fetch operations
  via the browser `fetch` API together with Installations tokens.
- When both `wasm-web` and `experimental-indexed-db` are enabled, Remote Config persists active templates, metadata, and
  custom signals into IndexedDB, mirroring the JS SDK’s storage behaviour across tabs and reloads.

## Still to do

- Native persistent storage defaults (e.g. file/IndexedDB selection) for mobile targets.
- Fetch throttling & resilience: persist throttle metadata, add exponential backoff, and expand error mapping to
  mirror `client/remote_config_fetch_client.ts` behaviour.
- Logging & errors: extend error surface (`ErrorCode` equivalents) and log-level tuning (`setLogLevel`).
- Realtime updates: support realtime update subscriptions (`client/realtime_handler.ts`).
- Testing parity: port fetch/activation/storage tests from `packages/remote-config/test` once functionality exists.

## Next Steps - Detailed Completion Plan

1. **Harden HTTP transport behaviour**
   - Persist throttle metadata/backoff information across restarts and mirror the JS SDK's throttle policies.
   - Extend fetch handling with richer error mapping (HTTP status → `RemoteConfigErrorCode`) and logging hooks.
2. **Add platform-specific persistent storage**
   - Finalize default selections for native/mobile and polish IndexedDB cleanup behaviour.
   - Mirror JS quota enforcement and test warm-up flows across restarts.
3. **Integrate logging controls and realtime updates**
   - Surface log-level configuration (`setLogLevel`) and map error codes that the JS SDK exposes.
   - Add realtime update subscriptions and ensure backoff metadata is persisted alongside fetch metadata.
