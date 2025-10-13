# Firebase Installations Port (Rust)

This directory contains the starter Rust implementation of the Firebase Installations SDK. The goal is to recreate the
functionality of `@firebase/installations` so other services can obtain Firebase Installation IDs (FIDs) and auth tokens.

## Current Functionality

- **Component wiring** – `register_installations_component` registers the public `installations` component so apps can
  call `get_installations` through the shared component system.
- **Installations service stub** – Generates an in-memory FID and auth token per app, supports `get_id` and
  `get_token(force_refresh)` with deterministic ID reuse and token rotation on refresh.
- **Token model** – Simple `InstallationToken` struct with token value and expiration timestamp.
- **Errors/constants** – Base error codes (`installations/invalid-argument`, `installations/internal`) and component
  name constant.
- **Tests** – Unit tests covering ID stability and forced token refresh.

This is enough for dependent modules to resolve an Installations instance and simulate token usage, but it lacks network
registration, persistence, and platform-specific functionality.

## Work Remaining (vs `packages/installations`)

1. **Network integration**
   - Call the Firebase Installations REST API to register installations, issue auth tokens, and handle errors/retries.
2. **Persistence**
   - Store FIDs and tokens using IndexedDB/localStorage with migration logic (mirroring `helpers/` and `util/` modules).
3. **Entry point parity**
   - Implement `deleteInstallations`, `getId`, `getToken`, and installation factory helpers with proper caching semantics.
4. **Authentication**
   - Include ETag handling, heartbeat integration, and auth headers required by the REST endpoints.
5. **Testing & emulator support**
   - Port unit/integration tests, including those for testing utilities and emulator flows.
6. **Platform differences**
   - Account for browser vs. node specifics (e.g., storage availability, fetch/eager initialization).
7. **Diagnostics**
   - Mirror JS logging, debug helpers, and analytics instrumentation present in the official SDK.

Implementing these items will bring the Rust Installations module to parity with the JavaScript SDK, providing reliable
FID/token management for the rest of the Firebase ecosystem.
