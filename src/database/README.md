# Firebase Realtime Database Port (Rust)

This directory contains the starting point for the Rust port of the Firebase Realtime Database SDK (`@firebase/database`).
The current implementation provides an in-memory stub so dependent modules can resolve the component and experiment with
basic read/write flows.

## Current Functionality

- **Component wiring** – `register_database_component` registers the `database` component allowing apps to obtain a
  `Database` instance via `get_database`.
- **In-memory data tree** – `DatabaseReference::set`, `update`, and `get` operate on a shared in-memory JSON tree,
  enabling hierarchical path manipulation (`child`, absolute paths).
- **Validation** – Basic path validation (reject empty segments) and error types (`database/invalid-argument`,
  `database/internal`).
- **Tests** – Unit tests covering set/get behaviour and child updates.

No network connectivity or persistence is provided; the data is lost once the process exits.

## Work Remaining (vs `packages/database`)

1. **SDK transport**
   - Implement WebSocket/REST protocol for realtime sync, streaming updates, and security rules evaluation.
2. **Authentication & security**
   - Integrate Auth/App Check tokens, security rules evaluation, and permission errors.
3. **Persistence & offline support**
   - Cache data locally, handle offline writes, and mirror multi-tab behaviour.
4. **Event listeners**
   - Support realtime listeners (`onValue`, `child_added`, etc.) with proper event queues and cancellation.
5. **Transactions & OnDisconnect**
   - Port transaction logic, on-disconnect handlers, and server values.
6. **Query operators**
   - Implement `orderBy*`, `startAt`, `limitToFirst`, etc., plus indexing helper APIs.
7. **Platform-specific adapters**
   - Handle browser/node differences (WebSocket availability, fetch, storage) similarly to the JS implementation.
8. **Testing parity**
   - Translate the extensive JS unit/integration tests and run them against the emulator.

Implementing these pieces will move the Rust Realtime Database module from an in-memory stub to a fully compatible SDK.
