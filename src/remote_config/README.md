# Firebase Remote Config Port (Rust)

This directory contains the early Rust port of Firebase Remote Config. The current implementation provides a minimal
in-memory stub so other modules can integrate with the component system and exercise the public API surface.

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
