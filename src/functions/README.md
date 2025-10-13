# Firebase Functions Port (Rust)

This directory contains the initial Rust scaffolding for the Firebase Cloud Functions (client) SDK. The objective is to
mirror `@firebase/functions` so apps can call HTTPS callable functions and other backends.

## Current Functionality

- **Component wiring** – `register_functions_component` exposes a public `functions` component allowing apps to resolve a
  `Functions` instance (optionally per-region) via the shared component container.
- **Functions service stub** – `https_callable` returns a `CallableFunction` that currently performs a local
  serialize/deserialize roundtrip, acting as a placeholder for real HTTP calls.
- **Region support** – Component factory honours instance identifiers for multi-region functions (defaults to
  `us-central1`).
- **Errors/constants** – Basic error types (`functions/internal`, `functions/invalid-argument`) and component name
  constant.
- **Tests** – Simple unit test that exercises `https_callable` to confirm data roundtrips.

This is sufficient for wiring and API shape, but it doesn’t actually invoke deployed Cloud Functions.

## Work Remaining (vs `packages/functions`)

1. **HTTPS callable transport**
   - Implement network layer (REST/Callable) including auth headers, App Check integration, timeout/retry logic, and
     data serialization using the functions serializer.
2. **Emulator support**
   - Honour emulator origin configuration and environment variables as in the JS SDK.
3. **Context and auth tokens**
   - Integrate with Auth/App Check to attach `Authorization` and `X-Firebase-AppCheck` headers automatically.
4. **Error handling parity**
   - Port error code mapping (HTTPS error codes to FunctionsError), custom error data, and cancellation behaviour.
5. **Function serialization**
   - Bring over the full serializer (handles Firestore Timestamps, GeoPoints, etc.) used for callable payloads.
6. **Modular helpers**
   - Implement helper functions for region selection, `httpsCallableFromURL`, and other convenience APIs.
7. **Testing**
   - Translate JS unit tests and add integration tests against the emulator for callable success/failure paths.

Completing these items will transition the Rust Functions module from a stub to a parity implementation capable of
calling deployed Cloud Functions.
