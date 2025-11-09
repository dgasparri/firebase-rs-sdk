## Porting status

- data_connect 80% `[########  ]`

## Implemented

- **Connector lifecycle** – Component registration mirrors `_getProvider('data-connect')`, supporting multi-instance caching keyed by `ConnectorConfig` plus optional env-based emulator routing.
- **Async service surface** – `DataConnectService` exposes async query/mutation execution, emulator toggling, and caller-sdk toggles with wasm-friendly futures.
- **HTTP transport parity** – REST transport builds the real `firebasedataconnect.googleapis.com` endpoints, appending API keys, `X-Goog-Api-Client`, Auth, and App Check headers plus GraphQL error mapping (`DataConnectOperationError`).
- **Query & mutation managers** – Rust versions of `QueryManager`/`MutationManager` track cache state, fan-out subscription callbacks, and hydrate from serialized snapshots.
- **Public helpers** – `query_ref`, `mutation_ref`, `execute_query`, `execute_mutation`, `subscribe`, and `to_query_ref` replicate the JS ergonomic helpers, returning strongly typed handles.
- **Tests** – Native integration tests use `httpmock` to verify query/mutation requests, subscription hydration, and emulator safeguards.

## Still to do

1. **Generated SDK utilities** – Hook `_useGeneratedSdk`/caller SDK types into higher-level generated bindings and expose public setters.
2. **Argument validation helpers** – Port `validateArgs`/input coercion so generated SDKs can transparently accept either `DataConnect` instances or raw variables.
3. **Streaming/subscription transports** – Implement websocket/subscription transports once the backend protocol is available.
4. **Retry/backoff policies** – Align REST retries with the JS SDK (currently single attempt aside from token refresh retries).
5. **Advanced logging & diagnostics** – Port `logDebug`/`logError` helpers and tie them into the shared logging framework.

## Next steps - detailed completion plan

1. **Expose generated SDK toggles**
   - Add public `use_generated_sdk()` and `set_caller_sdk_type()` wrappers plus doc examples so higher-level bindings can flip telemetry bits without touching internals.
   - Thread these settings through builder-style APIs so they can be set before the transport initializes.
2. **Input validation helper parity**
   - Port `validateArgs` from `packages/data-connect/src/util/validateArgs.ts`, accepting either a `DataConnectService` or raw variable map.
   - Integrate the helper into future generated bindings and document its usage for manual consumers.
3. **Transport robustness**
   - Mirror JS retry policies (e.g., retry on UNAUTH with refreshed tokens, exponential backoff on transient errors).
   - Add structured logging hooks around request/response boundaries to aid debugging, reusing the shared logging crate.
4. **Preparatory streaming work**
   - Define a trait for streaming transports so when subscriptions land we only need to plug implementations, keeping wasm/native parity in mind.
