# Firebase App module

This module ports core pieces of the Firebase App SDK to Rust.

The Firebase App coordinates the communication between the different Firebase components.

## Porting status

- app 60% \[######    \]

==As of October 20th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/app and the ported files in Rust in ./src/app, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

I’m working out that the Rust port covers about 60% of the major features based on careful comparison with the
  TypeScript source. I’ll list key supported features with file references like src/app/api.rs:1, note missing ones
  like IndexedDB support, and include a balanced summary explaining how the port hits core patterns but lacks some
  environment-specific details. This detailed yet cautious estimate will help clarify the port’s current maturity.

Ported coverage is about 60 %.

  - Core app lifecycle (initialize_app, get_app, get_apps, delete_app, register_version, set_log_level, on_log)
  is implemented with close parity to the JS API (src/app/api.rs:1, src/app/types.rs:1, src/app/registry.rs:1).
  Component registration, platform logger, and the in-memory heartbeat service mirror the JS structure (src/app/
  core_components.rs:1, src/app/platform_logger.rs:1, src/app/heartbeat.rs:1, src/app/logger.rs:1).
  - Rust also exposes the namespace bridge and internal component hooks that other services rely on, matching much of
  packages/app/src/api.ts:1, packages/app/src/internal.ts:1, packages/app/src/platformLoggerService.ts:1, and packages/
  app/src/heartbeatService.ts:1.
  - Missing or simplified pieces include IndexedDB-backed heartbeat persistence (packages/app/src/indexeddb.ts:1 has
  no Rust counterpart), full browser/environment detection and automatic config bootstrapping (getDefaultAppConfig,
  isBrowser, isWebWorker stubs in src/app/types.rs:180), richer server-app lifecycle (FinalizationRegistry, release-
  on-deref handling, JSON serialization from packages/app/src/firebaseServerApp.ts:1), detailed IDB error surfaces
  (packages/app/src/errors.ts:1), and the exhaustive JS test suite.
  - The Rust heartbeat service currently stores data only in-memory and omits async size pruning helpers that exist
  in the JS implementation (src/app/heartbeat.rs:1 vs. packages/app/src/heartbeatService.ts:1), and core-component
  registration skips build-target variants (packages/app/src/registerCoreComponents.ts:1).

  Given that the foundational runtime, component model, logging, and most public APIs are in place while platform-
  specific persistence, environment helpers, and several edge-case behaviours are still absent or stubbed, ~60 %
  reflects the practical parity today.

## References to the Firebase JS SDK - firestore module

- API: <https://firebase.google.com/docs/reference/js/app.md#app_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/app>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/app>

## Development status as of 14th October 2025

- Core functionalities: Mostly implemented (see the module's [README.md](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/firestore) for details)
- Tests: 12 tests (passed)
- Documentation: Most public functions are documented
- Examples: None provided

DISCLAIMER: This is not an official Firebase product, nor it is guaranteed that it has no bugs or that it will work as intended.

## Example Usage

```rust
use firebase_rs_sdk_unofficial::app::*;

fn main() -> AppResult<()> {
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
    let app = initialize_app(options, Some(settings))?;
    println!(
        "Firebase app '{}' initialised (project: {:?})",
        app.name(),
        app.options().project_id
    );

    // You can look the app up later using its name.
    let resolved = get_app(Some(app.name()))?;
    println!("Resolved app '{}' via registry", resolved.name());

    // The registry can also enumerate every active app.
    let apps = get_apps();
    println!("Currently loaded apps: {}", apps.len());
    for listed in apps {
        println!(
            " - {} (automatic data collection: {})",
            listed.name(),
            listed.automatic_data_collection_enabled()
        );
    }

    // When finished, delete the app to release resources and remove it from the registry.
    delete_app(&app)?;
    println!("App '{}' deleted", app.name());

    Ok(())
}
```


## What’s Implemented

- **Core API surface** (`api.rs` / `namespace.rs`)
  - `initialize_app`, `get_app`, `get_apps`, `delete_app`, and server-app stubbed entry points.
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
  - Bridges for internal modules to reach app types without exposing them publicly.

These pieces are enough for other Rust modules (Auth, Storage, Firestore) to register components and resolve `FirebaseApp`
instances.

## Gaps vs `packages/app`

The TypeScript module still includes functionality that is only partially or not yet ported:

1. **IndexedDB persistence & heartbeat**
   - `heartbeat.rs` provides an in-memory heartbeat service, but we still need the IndexedDB-backed storage and quota management implemented by `indexeddb.ts`.
2. **Platform logger service integration**
   - The platform logger is now registered, yet richer metadata (environment-specific variants, SDK aggregation) still needs parity with `platformLoggerService.ts`.
3. **App check for server environments**
   - `firebaseServerApp.ts` logic (hash-based naming, environment detection) is still a placeholder.
4. **Deferred component registration updates**
   - JS `registerCoreComponents.ts` re-registers components when new ones are added after app initialization. The Rust
     `initialize_app` path still clones components from the registry instead of reacting to updates.
5. **Advanced option comparison & validation**
   - Deep equality, whitespace normalization, and measurement/auto collection logic are simplified vs. JS version.
6. **Heartbeat/shared storage**
   - No equivalent of the JS public heartbeat APIs or persistence for client analytics.
7. **Testing parity**
   - TS suite covers edge cases (duplicate apps, app deletion, namespace behaviour) not yet mirrored in Rust tests.

## Next Steps

### Detailed Completion Plan

1. **Heartbeat subsystem**
   - Extend the new heartbeat service with IndexedDB-backed persistence and header construction logic that matches the JS algorithm (payload size limits, grouping, cleanup). Provide native fallbacks as needed and expose the header API to consumers.

2. **Platform logger service & version components**
   - Implement `PlatformLoggerServiceImpl` so the app container can produce the Firebase user-agent string. Ensure `register_version` registers a `ComponentType::Version` component (not just storing strings) to align with JS.
   - Add a Rust equivalent to `registerCoreComponents`, invoked during module init, to register both platform logger and heartbeat services and to call `register_version` for the core bundle.

3. **Server app parity**
   - Replace the `initialize_server_app` stub with the logic from `firebaseServerApp.ts`: environment checks, hashed naming, FinalizationRegistry/ref-count handling, token TTL validation, and version registration (`serverapp` variant). Extend `FirebaseServerApp` model with ref counting and cleanup semantics, plus `_is_firebase_server_app` helpers.

4. **Internal API coverage**
   - Expose equivalents of `_addComponent`, `_addOrOverwriteComponent`, `_clearComponents`, `_getProvider`, `_removeServiceInstance`, and `_isFirebaseServerApp`. Update tests to use them and ensure registry broadcasts new components to existing apps.
   - Fix the `'app'` component registration to return the actual `FirebaseApp` (matching JS `Component('app', () => this, …)`).

5. **Option/environment helpers**
   - Implement `get_default_app_config`, `is_browser`, `is_web_worker`, and deep equality utilities so auto-initialization and duplicate-detection match JS behaviour (including optional measurement ID handling and whitespace normalization).

6. **Testing parity**
   - Port `api.test.ts`, `firebaseApp.test.ts`, `firebaseServerApp.test.ts`, `internal.test.ts`, `heartbeatService.test.ts`, `indexeddb.test.ts`, and `platformLoggerService.test.ts` into Rust unit tests using the shared `test_support` scaffolding. Stub IndexedDB/browser-specific pieces with `wasm-bindgen-test` where appropriate.

7. **Documentation & examples**
   - Update README/examples once the above functionality lands, documenting server-app usage, heartbeat integration, and platform logging hooks.

### Recent Progress
- Registered the `platform-logger` and `heartbeat` components in Rust, delivering in-memory heartbeat storage and automatic version reporting to match the JS core registration flow (`src/app/core_components.rs`, `src/app/heartbeat.rs`, `src/app/platform_logger.rs`).
- Ported core `initializeApp`/`getApp`/`deleteApp` scenarios from the JS test suite to Rust, covering duplicate detection, component registration, version registration, and teardown behaviour (`src/app/api.rs`).

Completing these items will bring the Rust app module to parity with `@firebase/app` and ensure downstream services behave
consistently across ports.
