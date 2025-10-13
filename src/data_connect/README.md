# Firebase Data Connect Port (Rust)

This directory hosts the early-stage Rust port of Firebase Data Connect. The aim is to mirror the modular JS SDK
(`@firebase/data-connect`) so applications can execute Data Connect queries through the shared component framework.

## Current Functionality

- **Component wiring** – `register_data_connect_component` registers a `data-connect` component allowing apps to retrieve a
  `DataConnectService` via `get_data_connect_service`.
- **Service stub** – `DataConnectService::execute` accepts a `QueryRequest` (operation text + variables) and returns a
  synthetic `QueryResponse` echoing the request and resolved endpoint.
- **Endpoint handling** – Supports per-endpoint instances via instance identifiers/options.
- **Errors/constants** – Basic error codes (`data-connect/invalid-argument`, `data-connect/internal`) and component name
  constant.
- **Tests** – Unit tests covering successful execution and empty-operation validation.

The module currently provides structural integration but does not talk to the real Data Connect backend.

## Work Remaining (vs `packages/data-connect`)

1. **Backend transport**
   - Implement network layer for GraphQL/REST calls, including authentication headers, retries, and result parsing.
2. **Schema & type helpers**
   - Port schema introspection, method generators, and request helpers from the JS SDK (`api/`, `core/`, `util/`).
3. **Config handling**
   - Support configuration fetch/refresh, project/endpoint resolution, and environment overrides.
4. **Error mapping & logging**
   - Mirror JS error handling, structured logging, and developer diagnostics.
5. **Streaming/subscriptions**
   - Implement subscription/websocket features once the base transport is in place.
6. **Testing parity**
   - Translate JS unit/integration tests (API, serializers, backend) and run against emulators/live endpoints.

Completing these steps will move the Rust Data Connect port from a stub to a fully functional client aligned with the
JavaScript SDK.
