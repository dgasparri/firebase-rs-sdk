# Firebase Functions Port (Rust)

This directory contains the initial Rust scaffolding for the Firebase Cloud Functions (client) SDK. The objective is to
mirror `@firebase/functions` so apps can call HTTPS callable functions and other backends.


## Porting status

- functions 5% \[#         \]

==As of October 20th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/functions and the ported files in Rust in ./src/functions, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

I'm assessing how much the Rust stub covers compared to the full JS implementation. It looks like the Rust side offers
  just minimal structure—component registration and basic stub calls returning the same data—while JS has rich features
  like HTTP calls, streaming, context handling, and multiple dependencies. Based on missing elements like network
  calls, emulator connection, auth tokens, serializers, error mapping, and tests (mostly in packages/functions/src/),
  I estimate coverage around 5%. I'll provide a summary listing the Rust files with implemented features and JS files
  showing missing functionality to clearly present the gaps.

Estimated coverage is about 5 % of the JS Functions SDK.

  - Present in Rust: component registration and retrieval plus a stub callable wrapper (src/functions/api.rs:1), basic
  error codes (src/functions/error.rs:1), constants (src/functions/constants.rs:1), and the README noting current scope
  (src/functions/README.md:1). Callable functions just echo the payload via JSON roundtrip—no transport or provider
  integration.
  - Missing: the full service implementation that handles fetch transport, auth/app-check/messaging headers, emulator
  wiring, timeouts, retries, and streaming (packages/functions/src/service.ts:1, packages/functions/src/context.ts:1);
  serialization helpers for Firestore types (packages/functions/src/serializer.ts:1); rich error mapping and custom
  details (packages/functions/src/error.ts:1); public API utilities like httpsCallableFromURL, connectFunctionsEmulator,
  and options handling (packages/functions/src/api.ts:1); configuration/emulator detection (packages/functions/src/
  config.ts:1); and the comprehensive test suite (packages/functions/src/callable.test.ts:1, etc.).

  Given that only the API skeleton exists without any network behavior, serialization, or platform integrations, the
  current Rust port covers roughly one-twentieth of the features provided by the JavaScript module.



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
