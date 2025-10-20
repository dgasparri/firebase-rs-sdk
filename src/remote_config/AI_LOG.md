# 12 October 2025

Added a stubbed Remote Config module plus new Performance and Remote Config services:

  - Remote Config: Component wiring, in-memory defaults/activation, value retrieval, caching, unit tests, and README
  (src/remote_config/*, src/lib.rs).
  - Performance: Component wiring, simple trace recording, tests, README (src/performance/*, src/lib.rs).
  - Updated Installations/Data Connect retrieval to maintain caches and ensure repeated lookups succeed in tests;
  Realtime Database stub added earlier. README files created for analytics, app(-check), auth, messaging, install,
  functions, AI, data_connect, database, performance, remote_config.


## 20 October 2025

> PROMPT: what about remote_configmessaging module? (previous prompt was: Now compare the original JS/Typescript files in ./packages/app and the ported files in Rust in ./src/app, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust)

Remote Config coverage sits at roughly 3 %.

  - Rust mirrors only the scaffolding: a component registration plus an in-memory RemoteConfig that stores defaults and
  copies them into active values on activate, while fetch is a no-op (src/remote_config/api.rs:1). Errors and constants
  exist, and tests cover default activation only.
  - The JS SDK is far richer, with initialization options, storage caching, network fetch via RemoteConfigFetchClient,
  realtime handlers, settings tweaks, typed getters (getBoolean, getNumber, etc.), custom signals, logging, fetch
  throttling, minimum intervals, persistence with Storage/IndexedDB, and realtime updates (packages/remote-config/src/
  api.ts:1, packages/remote-config/src/remote_config.ts:1, and the accompanying client, storage, value, errors, register
  directories). None of that complexity—networking, caching, realtime listeners, settings—is present in Rust.

  Given the stub retains only the most basic API surface without any real backend interaction, settings, persistence, or
  typed value accessors, the Rust port amounts to about 3 % of the functionality of the JavaScript module.