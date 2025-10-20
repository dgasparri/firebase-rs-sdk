## 20 October 2025

> PROMPT: what about the installations module? (previous prompt was: Now compare the original JS/Typescript files in ./packages/app and the ported files in Rust in ./src/app, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust)

 I’m reviewing the Rust and JS implementations to compare their feature coverage. The Rust code is a minimal stub with
  basic ID and token generation stored in memory, lacking persistence, network calls, and component registration seen
  in JS. Many features like deleteInstallations, onIdChange, multi-app support, and caching are missing, so I estimate
  coverage around 5%. I’ll reference specific file paths and lines to support this analysis and highlight the biggest
  gaps before crafting the final summary.

> Coverage is around 5 %.

  - What exists in Rust is just a stub: component registration plus an in-memory Installations service that fabricates
  a random FID/token per app (src/installations/api.rs:1) with basic error types (src/installations/error.rs:1) and the
  constant definition (src/installations/constants.rs:1). No persistence, retries, or network calls are attempted.
  - The JS SDK is vastly richer. It registers both public and internal components, extracts validated app config, wires
  in heartbeat, and shares the implementation with other modules (packages/installations/src/functions/config.ts:1). It
  persists installation entries in IndexedDB, handles concurrency, and respects spec-compliant FID generation with fid-
  changed notifications (packages/installations/src/helpers/get-installation-entry.ts:1, packages/installations/src/
  helpers/generate-fid.ts:1, packages/installations/src/helpers/fid-changed.ts:1, packages/installations/src/helpers/
  idb-manager.ts:1).
  - Token management and REST integration are entirely missing: the JS port handles registration and auth token
  refresh via network calls with ETag logic and retry/backoff policies (packages/installations/src/helpers/refresh-
  auth-token.ts:1) and exposes deleteInstallations, getId, getToken, and onIdChange with proper lifecycle guarantees
  (packages/installations/src/api/delete-installations.ts:1, packages/installations/src/api/get-id.ts:1, packages/
  installations/src/api/get-token.ts:1, packages/installations/src/api/on-id-change.ts:1).
  - There’s no internal interface for other services, no emulator or diagnostics support, and the extensive helper/util/
  test suites haven’t been ported (packages/installations/src/util/*, packages/installations/src/testing/*).

  Given that the Rust module only mirrors the surface types and returns dummy data while the JS implementation covers
  persistence, networking, retries, events, and internal wiring, the current port implements about one-twentieth of
  the functionality.
