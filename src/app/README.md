# Firebase App Port (Rust)

This directory contains the current Rust port of the Firebase App module. The goal is to reproduce the behaviour of
`@firebase/app` while integrating cleanly with the shared component system used by the rest of the crate.

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
   - `indexeddb.ts` and `heartbeatService.ts` manage heartbeat storage/reporting; currently missing in the Rust port.
2. **Platform logger service integration**
   - `platformLoggerService.ts` and dynamic platform logging metadata are stubbed/not wired.
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

1. **Heartbeat & platform services**
   - Port `heartbeatService` and `platformLoggerService`, wiring them into app initialization and the component registry.
2. **Server app parity**
   - Implement `initializeServerApp` fully (hash naming, token handling, cleanup semantics) to match `firebaseServerApp.ts`.
3. **Dynamic component registration**
   - Update `registry.rs`/`initialize_app` so apps subscribe to new global components registered after initialization.
4. **Option validation enhancements**
   - Bring over the deep/default comparison helpers from `types.ts` to ensure behaviour matches JS, especially around
     measurement ID and auto data collection.
5. **IndexedDB persistence hooks**
   - Port the IndexedDB cache used for heartbeat data and other app-level persistence.
6. **Comprehensive tests**
   - Translate `api.test.ts`, `firebaseApp.test.ts`, and related suites to Rust unit tests.
7. **Documentation & examples**
   - Extend README/examples to show usage once the remaining features land.

Completing these items will bring the Rust app module to parity with `@firebase/app` and ensure downstream services behave
consistently across ports.
