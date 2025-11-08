## Porting status

- functions 25% `[###       ]`

(Estimated after landing the callable transport, context headers, and persistence plumbing on
October 20th, 2025.)


## Implemented

- Component registration so `Functions` instances can be resolved from a `FirebaseApp` container.
- Native HTTPS callable transport backed by async `reqwest::Client`, exposed through an async
  transport layer.
- Public callable API is async-only so the module works seamlessly on native and WASM targets.
- Error code mapping aligned with the JS SDK (`packages/functions/src/error.ts`) including backend
  `status` translation and message propagation.
- Custom-domain targeting (including emulator-style origins) by interpreting the instance
  identifier passed to `get_functions`.
- Unit test (`https_callable_invokes_backend`) that validates request/response wiring against an
  HTTP mock server (skips automatically when sockets are unavailable).
- Context provider that pulls Auth and App Check credentials from the component system and injects
  the appropriate `Authorization` / `X-Firebase-AppCheck` headers on outgoing callable requests.
- WASM transport that issues callable requests through `fetch`, mirrors native timeout semantics
  with `AbortController`, and reuses shared error mapping.
- Automatic FCM token lookup so callable requests include the
  `Firebase-Instance-ID-Token` header when the Messaging component is configured and a cached (or
  freshly resolved) token is available.

## Still to do

- Serializer parity for Firestore Timestamp, GeoPoint, and other special values used by callable
  payloads (`packages/functions/src/serializer.ts`).
- Emulator helpers (`connectFunctionsEmulator`) and environment detection to configure the base
  origin (`packages/functions/src/service.ts`).
- Streaming callable support (`httpsCallable().stream`) that handles server-sent events.
- Public helpers like `httpsCallableFromURL` and region selection utilities from
  `packages/functions/src/api.ts`.
- Comprehensive error detail decoding (custom `details` payload) and cancellation handling.
- Broader test coverage mirroring `packages/functions/src/callable.test.ts`.
- Restore Firebase Messaging token forwarding on WASM once the messaging module exposes a wasm-web
  implementation.

## Next steps - Detailed completion plan

1. **Serializer parity & URL utilities**
   - Port the callable serializer helpers (Firestore timestamps, GeoPoints) and add helpers such as
     `https_callable_from_url` and `connect_functions_emulator` for full API coverage.
2. **Wasm validation & tooling**
   - Stand up `wasm-bindgen-test` coverage to verify the fetch transport, timeout handling, and
     header propagation in a browser-like environment.
   - Document the crate features (`wasm-web`) and any required Service Worker setup in the README
     example section.
3. **Advanced error handling**
   - Surface backend `details` payloads and cancellation hooks so callers can differentiate retryable
     vs. fatal failures, matching `packages/functions/src/error.ts` semantics.
4. **Async adoption sweep**
   - Audit the workspace for callers of `get_functions` and `CallableFunction::call_async`, updating
     each site to await the async APIs and adjusting examples/docs where necessary.
   - Re-run the smoke scripts (`scripts/smoke.sh` / `.bat`) to confirm both native and wasm builds
     stay green after the sweep.
