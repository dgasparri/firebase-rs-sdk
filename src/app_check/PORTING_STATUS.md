## Porting status

- app_check 70% `[#######   ]`

Significant parity milestones are now in place: App Check registers with the component system, background refresh follows the JS proactive-refresh heuristics (issued/expiry timestamps, jitter, exponential backoff), tokens persist across reloads on wasm targets, and storage, analytics, and other modules can request App Check tokens via the shared internal provider. ReCAPTCHA flows, debug tooling, and heartbeat integration remain unported, but the primary token lifecycle is functional and covered by tests.


## Implemented

- **Component registration & interop** (`api.rs`, `interop.rs`)
  - Public and internal App Check components register with the Firebase component system so other services can obtain tokens via `FirebaseAppCheckInternal`.
- **Token lifecycle management** (`state.rs`, `api.rs`)
  - In-memory cache with listener management, limited-use token support, and graceful error propagation when refreshes fail but cached tokens remain valid.
- **Proactive refresh scheduler** (`refresher.rs`, `api.rs`)
  - Matches the JS `proactive-refresh` policy (midpoint + 5 min offset, exponential backoff, cancellation) and automatically starts/stops based on auto-refresh settings.
- **Persistence & cross-tab broadcast** (`persistence.rs`)
  - IndexedDB persistence for wasm builds now records issued and expiry timestamps and broadcasts token updates across tabs; native builds fall back to memory cache.
- **Heartbeat telemetry** (`types.rs`, `api.rs`)
  - Integrates with the shared heartbeat service so outgoing requests can attach the `X-Firebase-Client` header (Storage, Firestore, Functions, Database, and AI clients now call through `FirebaseAppCheckInternal::heartbeat_header`).
- **Tests & tooling** (`api.rs`, `interop.rs`, `token_provider.rs`, `storage/service.rs`)
  - Unit tests cover background refresh, cached-token error handling, internal listener wiring, and Storage integration; shared test helpers ensure state isolation.
- **reCAPTCHA v3 & Enterprise providers** (`providers.rs`, `client.rs`, `recaptcha.rs`)
  - Script bootstrap, invisible widget lifecycle, attestation exchange, and throttling semantics match the JS SDK. Native builds still surface configuration errors, while wasm builds load Google’s scripts on demand.

## Still to do

- Debug token developer mode, emulator toggles, and console logging parity.
- Web-specific visibility listeners and throttling heuristics (document visibility, pause on hidden tabs).
- Broader provider catalogue (App Attest, SafetyNet) and wasm-friendly abstractions for platform bridges.


## Intentional deviations

- **No dummy-token fallback** – The JS SDK always resolves `getToken()` with a string, returning a base64 "dummy" token alongside error metadata when the exchange fails. Rust callers already rely on `Result`, so the port surfaces enriched error variants instead of fabricating placeholder tokens. This keeps downstream code explicit while still exposing throttling/backoff details through the returned error value.


## Next steps – Detailed completion plan

1. **Debug/emulator workflow**
   - Persist debug tokens, expose APIs to toggle debug mode, and surface console hints mirroring `debug.ts`; ensure emulator host/port wiring is available to downstream services.
2. **Internal API parity**
   - Finalise the remaining `internal-api.ts` flows—most notably the debug-token exchange and associated console hints—so limited-use requests, throttling callbacks, and developer tooling fully match the JS SDK.
3. **Visibility-aware refresh controls**
   - Add document visibility listeners on wasm targets and equivalent hooks for native platforms so refresh pauses/resumes follow the JS scheduler behaviour.
4. **Expand tests & docs**
   - Backfill the JS unit scenarios (refresh retry tables, storage integration failures) and extend rustdoc/README guidance, including wasm-specific notes and provider examples.

### Debug mode plan

debug mode in app_check and other modules
  1. Debug-mode path
      - Port the debug helpers from packages/app-check/src/debug.ts: keep a per-app debug token (getDebugToken,
  setDebugToken), persist it in IndexedDB for wasm, and expose the developer toggle API.
      - Teach get_limited_use_token (and get_token) to branch: if debug mode is active, call our REST client’s
  exchange_token with get_exchange_debug_token_request, bypassing the provider entirely, then write the new token back
  into state/storage so later calls hit the cached value.
  2. Shared state integration
      - Extend state::AppCheckState to hold the pending debug exchange future the way JS does (exchangeTokenPromise), so
  multiple requests coalesce and you don’t spam the backend.
      - Make sure tokens gained through the debug flow raise the same refresh/listener events as provider-issued ones.
  3. Error wrapping & metadata
      - When the debug exchange fails, return the same AppCheckTokenError we now use for provider errors:
          - TokenErrorKind::Throttled when the backend returns 429/503 with Retry-After.
          - TokenErrorKind::Soft if a previously cached token is still valid.
          - TokenErrorKind::Fatal otherwise.
      - That keeps Storage/Firestore/Functions behaving exactly like the JS SDK, just without dummy strings.

  Supporting work: wire the debug toggle into public API (e.g., set_app_check_debug_token), add persistence for wasm
  (mirroring writeDebugTokenToStorage), update tests to cover both branches, and refresh the README/docs so developers
  know how to enable debug mode in Rust.

  Once those pieces are in place, limited-use tokens behave the same as JS: developer mode “just works”, throttling/
  backoff signals propagate correctly, and there’s still no dummy token.
