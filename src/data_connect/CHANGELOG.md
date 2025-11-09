# Firebase Performance

## From v1.27.1

- **Connector lifecycle** – Component registration mirrors `_getProvider('data-connect')`, supporting multi-instance caching keyed by `ConnectorConfig` plus optional env-based emulator routing.
- **Async service surface** – `DataConnectService` exposes async query/mutation execution, emulator toggling, and caller-sdk toggles with wasm-friendly futures.
- **HTTP transport parity** – REST transport builds the real `firebasedataconnect.googleapis.com` endpoints, appending API keys, `X-Goog-Api-Client`, Auth, and App Check headers plus GraphQL error mapping (`DataConnectOperationError`).
- **Query & mutation managers** – Rust versions of `QueryManager`/`MutationManager` track cache state, fan-out subscription callbacks, and hydrate from serialized snapshots.
- **Public helpers** – `query_ref`, `mutation_ref`, `execute_query`, `execute_mutation`, `subscribe`, and `to_query_ref` replicate the JS ergonomic helpers, returning strongly typed handles.
- **Tests** – Native integration tests use `httpmock` to verify query/mutation requests, subscription hydration, and emulator safeguards.
