## Porting status

- messaging 40% `[####      ]`

## Implemented

- Component registration that exposes `Messaging` through the shared component system.
- In-memory token management for `get_token` and `delete_token` with validation of VAPID keys.
- `messaging::is_supported()` guard that mirrors `packages/messaging/src/api/isSupported.ts` for web builds.
- Asynchronous `Messaging::request_permission` flow that awaits `Notification.requestPermission` and surfaces
  `messaging/permission-blocked` and `messaging/unsupported-browser` errors.
- `ServiceWorkerManager` helper that registers the default Firebase messaging worker (mirrors
  `helpers/registerDefaultSw.ts`) and caches the resulting registration for reuse.
- WASM-only `PushSubscriptionManager` that wraps `PushManager.subscribe`, returns subscription details and supports
  unsubscribe/error mapping aligned with the JS SDK implementation.
- Token persistence layer that stores token metadata (including subscription details and creation time) per app,
  mirroring the IndexedDB-backed token manager with IndexedDB on wasm (plus BroadcastChannel sync) when the
  `experimental-indexed-db` feature is enabled, and falling back to in-memory storage on native/other builds. Weekly
  refresh logic invalidates tokens after 7 days to trigger regeneration.
- WASM builds now reuse the async Installations component to cache real FID/refresh/auth tokens alongside the
  messaging token store and perform real FCM registration/update/delete calls.
- FCM REST requests share an exponential backoff (429/5xx aware) retry strategy to mirror the JS SDK behaviour.
- Minimal error types for invalid arguments, internal failures and token deletion.
- Non-WASM unit test coverage for permission/token flows, invalid arguments, token deletion, service worker and push
  subscription stubs.

## Still to do

- Track permission changes across sessions and expose notification status helpers similar to the JS SDK.
- Call the Installations and FCM REST endpoints to create, refresh and delete tokens, including weekly refresh checks
  (currently available only when `experimental-indexed-db` is enabled). Review the async/wasm client work in
  `src/installations` for reusable patterns before wiring the Messaging flows.
- Provide richer fallbacks when `experimental-indexed-db` is disabled (e.g. in-memory tokens per tab) so the wasm
  build can still obtain tokens without IndexedDB support.
- Coordinate multi-tab state and periodic refresh triggers using IndexedDB change listeners (BroadcastChannel / storage events).
- Foreground/background message listeners, payload decoding and background handlers.
- Environment-specific guards (SW vs window scope), emulator/testing helpers and extended error coverage.

## Next steps - Detailed completion plan

1. Harden the new FCM REST integration with richer retry/backoff logic and unit tests that model server-side failures (mirroring `requests.test.ts`).
2. Add multi-tab coordination (BroadcastChannel/storage events) so service worker updates fan out to every context and avoid duplicate registrations.
3. Port message delivery APIs (`onMessage`, `onBackgroundMessage`) and event dispatchers, including WASM gating for background handlers.
4. Expand the error catalog to match `packages/messaging/src/util/errors.ts`, update documentation and backfill tests for the newly added behaviours.
