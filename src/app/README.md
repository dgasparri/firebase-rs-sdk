# Firebase App module

This module ports core pieces of the Firebase App SDK to Rust.

The Firebase App coordinates the communication between the different Firebase components.

## Porting status

- app 80% `[########  ]`

_As of October 24th, 2025_

- The async app lifecycle (`initialize_app`, `get_app`, `get_apps`, `delete_app`, version registration, logging)
  mirrors `packages/app/src/api.ts`, including hashed naming for server apps and duplicate-config detection.
- Environment helpers now expose `get_default_app_config`, `is_browser`, and `is_web_worker`, sourcing configuration
  from `__FIREBASE_DEFAULTS__`, `FIREBASE_CONFIG`, and related overrides so the Rust port can auto-bootstrap like the
  JS SDK on both native and WASM targets.
- Server-side apps support `release_on_deref` semantics via deferred deletion, matching the JS `FirebaseServerApp`'s
  FinalizationRegistry behaviour while continuing to validate auth/app-check tokens.
- Internal component utilities (`add_component`, `add_or_overwrite_component`, `get_provider`, `remove_service_instance`,
  `clear_components`, `register_component`) are re-exported through `app::private`, mirroring
  `packages/app/src/internal.ts` for downstream modules.

## References to the Firebase JS SDK - firestore module

- API: <https://firebase.google.com/docs/reference/js/app.md#app_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/app>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/app>

## Development status as of 24th October 2025

- Core functionalities: Mostly implemented 
- Tests: 19 unit tests (passing)
- Documentation: Most public functions are documented
- Examples: None provided

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.

## Example Usage

```rust,no_run
use firebase_rs_sdk::app::*;

#[tokio::main(flavor = "current_thread")]
async fn main() -> AppResult<()> {
    // Configure your Firebase project credentials. These values are placeholders that allow the
    // example to compile without contacting Firebase services.
    let options = FirebaseOptions {
        api_key: Some("demo-api-key".into()),
        project_id: Some("demo-project".into()),
        storage_bucket: Some("demo-project.appspot.com".into()),
        ..Default::default()
    };

    // Give the app a custom name and enable automatic data collection.
    let settings = FirebaseAppSettings {
        name: Some("demo-app".into()),
        automatic_data_collection_enabled: Some(true),
    };

    // Create (or reuse) the app instance.
    let app = initialize_app(options, Some(settings)).await?;
    println!(
        "Firebase app '{}' initialised (project: {:?})",
        app.name(),
        app.options().project_id
    );

    // You can look the app up later using its name.
    let resolved = get_app(Some(app.name())).await?;
    println!("Resolved app '{}' via registry", resolved.name());

    // The registry can also enumerate every active app.
    let apps = get_apps().await;
    println!("Currently loaded apps: {}", apps.len());
    for listed in apps {
        println!(
            " - {} (automatic data collection: {})",
            listed.name(),
            listed.automatic_data_collection_enabled()
        );
    }

    // When finished, delete the app to release resources and remove it from the registry.
    delete_app(&app).await?;
    println!("App '{}' deleted", app.name());

    Ok(())
}
```

> **Runtime note:** The example uses `tokio` as the async executor for native builds. When targeting WASM, rely on
> `wasm-bindgen-futures::spawn_local` or the host runtime instead.


## What’s Implemented

- **Core async API surface** (`api.rs` / `namespace.rs`)
  - `initialize_app`, `get_app`, `get_apps`, `delete_app`, and server-app entry points now expose async-first signatures that mirror the Firebase JS naming.
  - Basic namespace exposure so other services can obtain the default app instance.
- **Component infrastructure** (`component.rs`, `registry.rs`)
  - Component container/provider abstractions and global registration logic that mirror the JS `ComponentContainer`.
  - Public `register_component` hook used by other ports (Auth, Storage, Firestore).
- **Error handling & constants** (`errors.rs`, `constants.rs`)
  - Ported error enums/messages covering duplicate apps, invalid names, missing options, etc.
  - Default entry name and platform logging constants.
- **Logging** (`logger.rs`)
  - Lightweight logger abstraction with global log level, mirroring the JS logging hooks.
- **Types** (`types.rs`)
  - `FirebaseApp`, `FirebaseAppSettings`, `FirebaseOptions`, and server-app scaffolding with option/config comparisons.
- **Private/internal exports** (`private.rs`)
  - Bridges for internal modules to reach app types without exposing them publicly, alongside helpers for component
    registration and provider management.
- **Environment helpers** (`types.rs`, `platform/environment.rs`)
  - `get_default_app_config`, `is_browser`, and `is_web_worker` mirror the JS detection logic, respecting
    `__FIREBASE_DEFAULTS__`, `FIREBASE_CONFIG`, and forced environment overrides for both native and WASM builds.
- **Server app lifecycle** (`api.rs`, `types.rs`)
  - Server apps honour `release_on_deref` by spawning deferred teardown through the registry, while retaining the
    hashed naming, reference counting, and token TTL validation implemented in JS.
- **Internal API surface** (`private.rs`)
  - `_addComponent`, `_addOrOverwriteComponent`, `_clearComponents`, `_getProvider`, `_removeServiceInstance`, and
    `_registerComponent` equivalents are exposed so other modules can mirror the JS internal API without duplicating
    registry logic.

These pieces are enough for other Rust modules (Auth, Storage, Firestore) to register components and resolve `FirebaseApp`
instances.

## Gaps vs `packages/app`

The TypeScript module still includes functionality that is only partially or not yet ported:

1. **IndexedDB-backed heartbeat persistence & headers**
   - `heartbeat.rs` still stores events in memory; we need the IndexedDB quota logic, pruning, and header construction
     from `packages/app/src/indexeddb.ts` and `heartbeatService.ts` to align with browser behaviour.
2. **Platform logger enrichment**
   - `PlatformLoggerServiceImpl` currently concatenates version components but omits the richer SDK metadata and user
     agent strings produced in `platformLoggerService.ts`.
3. **Browser auto-configuration**
   - While environment detection is in place, parsing config from script tags/cookies, measurement ID normalization,
     and whitespace trimming from `getDefaultAppConfig` remain unported.
4. **Server app advanced APIs**
   - `initialize_server_app` still lacks the overload that accepts an existing `FirebaseApp`, the `toJSON` stub, and
     finer-grained release controls covered in `firebaseServerApp.ts`.
5. **Testing parity**
   - The JS suite includes extensive coverage (heartbeat, platform logger, indexedDB failure modes) that still needs
     to be reflected in Rust unit and integration tests.

## Next Steps

### Detailed Completion Plan

1. **Heartbeat persistence & headers**
   - Port the IndexedDB storage, payload trimming, and header serialization from `heartbeatService.ts`, exposing a
     public API for consumers while keeping native fallbacks compatible.

2. **Platform logger enrichment**
   - Extend `PlatformLoggerServiceImpl` to build the Firebase user-agent string (bundling SDK identifiers, platform
     hints, and variants) and ensure eager registration mirrors `registerCoreComponents.ts`.

3. **Browser auto-bootstrap**
   - Parse config from script tags and cookies, normalize measurement IDs, and port whitespace/undefined guards so
     `get_default_app_config` matches the JS heuristics.

4. **Server app API parity**
   - Add the overload that accepts an existing `FirebaseApp`, port the `toJSON` behaviour, and expose ergonomic
     wrappers for explicit release so server environments behave exactly like `firebaseServerApp.ts`.

5. **Testing & documentation**
   - Port the remaining JS tests (heartbeat/indexedDB/platform logger/server app) and expand module docs/examples to
     cover the new configuration and server-side flows.

### Recent Progress
- Hooked `get_default_app_config`, `is_browser`, and `is_web_worker` into the build so native and WASM targets reuse
  environment-driven defaults instead of stubs.
- Added release-on-drop semantics for `FirebaseServerApp`, updated `delete_app` to manage server refs, and verified the
  behaviour with async tests.
- Re-exported the JS internal component helpers (`_addComponent`, `_getProvider`, etc.) through `app::private` and
  added coverage to exercise registry propagation and service eviction.
- Expanded unit tests for server apps and private helpers to guard future regressions.

Completing these items will bring the Rust app module to parity with `@firebase/app` and ensure downstream services behave
consistently across ports.
