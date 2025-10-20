# Firebase Remote Config Port (Rust)

This directory contains the early Rust port of Firebase Remote Config. The current implementation provides a minimal
in-memory stub so other modules can integrate with the component system and exercise the public API surface.

## Porting status

- remote_config 3% \[          \]

==As of October 20th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/remote_config and the ported files in Rust in ./src/remote_config, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust


Remote Config coverage sits at roughly 3 %.

  - Rust mirrors only the scaffolding: a component registration plus an in-memory RemoteConfig that stores defaults and
  copies them into active values on activate, while fetch is a no-op (src/remote_config/api.rs:1). Errors and constants
  exist, and tests cover default activation only.
  - The JS SDK is far richer, with initialization options, storage caching, network fetch via RemoteConfigFetchClient,
  realtime handlers, settings tweaks, typed getters (getBoolean, getNumber, etc.), custom signals, logging, fetch
  throttling, minimum intervals, persistence with Storage/IndexedDB, and realtime updates (packages/remote-config/src/
  api.ts:1, packages/remote-config/src/remote_config.ts:1, and the accompanying client, storage, value, errors, register
  directories). None of that complexity—networking, caching, realtime listeners, settings—is present in Rust.

Given the stub retains only the most basic API surface without any real backend interaction, settings, persistence, or typed value accessors, the Rust port amounts to about 3 % of the functionality of the JavaScript module.

## Current Functionality

- **Component wiring** – `register_remote_config_component` registers the `remote-config` component, allowing
  `get_remote_config` to resolve a `RemoteConfig` instance through the shared container.
- **Defaults & activation** – `set_defaults`, `fetch`, and `activate` update an in-memory store; activate copies defaults
  into the active map once.
- **Value retrieval** – `get_string` returns active or default values for a key.
- **Caching** – Simple process-level cache keyed by app name ensures idempotent lookups.
- **Tests** – Unit tests cover default activation and double-activate behaviour.

No network fetch or backed storage is performed; values are lost when the process exits.

## Work Remaining (vs `packages/remote-config`)

1. **Fetch/activate transport**
   - Implement REST interaction with the Remote Config backend, including throttling, caching headers, and ETags.
2. **Storage & persistence**
   - Cache templates/activated values on disk (IndexedDB/localStorage) with TTL support and multi-tab coordination.
3. **Settings & minimum fetch interval**
   - Port settings management (`setConfigSettings`, `minimumFetchIntervalMillis`, etc.).
4. **Default configuration helpers**
   - Mirror JS helpers for converting defaults from objects/files and typed accessors (`getBoolean`, `getNumber`).
5. **Logging & error handling**
   - Map backend errors, throttle reasons, and logging utilities (`logger.ts`).
6. **Testing parity**
   - Translate JS tests for fetch/activate flows, storage behaviour, and settings edge cases.

Implementing these steps will transform the stub into a functional Remote Config client aligned with the JavaScript SDK.
