# Firebase Data Connect Port (Rust)

This module contains the early-stage Rust port of Firebase Data Connect. The goal is to mirror the modular JS SDK
(`@firebase/data-connect`) so applications can execute Data Connect queries asynchronously through the shared component
framework.

## Porting status

- data_connect 5% `[#         ]`

==As of October 21th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/data_connect and the ported files in Rust in ./src/data_connect, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module

Thinking
  I’m putting together a clear final answer that highlights the limited cross-implementation coverage, estimating around
  5% port completion based on comparing Rust and JS features. I'll reference exact file lines like src/data_connect/
  api.rs:17 and packages/data-connect/src/api/DataConnect.ts:99 to back this up and mention major missing features,
  wrapping it all nicely in bullet points. Next steps might come up naturally.

Port Estimate

  - Roughly 5 % of the JS Data Connect SDK is represented in Rust. The current port stops at component registration and
  a stubbed execution path (src/data_connect/api.rs:17), plus simple constants/errors (src/data_connect/constants.rs:1,
  src/data_connect/error.rs:4).
  - The JS package layers in full client behavior—instance management, emulator wiring, auth & App Check providers,
  transport selection, query/mutation managers, caching, and subscription flows (packages/data-connect/src/api/
  DataConnect.ts:99, packages/data-connect/src/core/QueryManager.ts:1, packages/data-connect/src/network/index.ts,
  packages/data-connect/src/api/query.ts:52). None of these mechanics exist in Rust yet.
  - Mutation helpers, serialization utilities, encoder/map tooling, and validation logic (packages/data-connect/src/
  api/Mutation.ts, packages/data-connect/src/util/encoder.ts, packages/data-connect/src/util/validateArgs.ts) are also
  unported, leaving the Rust API unable to mirror real backend calls or generated SDK hooks.

Natural next steps: 1) Implement real transports (REST/emulator) with auth/App Check headers so DataConnectService::execute can hit live endpoints; 2) Port QueryManager/MutationManager and supporting types to provide query refs, caching, and subscription semantics; 3) Mirror option validation and multi-instance handling so the Rust façade aligns with the JS modular API surface.


## Quick Start Example

```rust,no_run
use std::collections::BTreeMap;

use firebase_rs_sdk::app::api::initialize_app;
use firebase_rs_sdk::app::{FirebaseAppSettings, FirebaseOptions};
use firebase_rs_sdk::data_connect::{
    get_data_connect_service, QueryRequest
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        project_id: Some("demo-project".into()),
        ..Default::default()
    };
    let settings = FirebaseAppSettings {
        name: Some("demo-app".into()),
        ..Default::default()
    };
    let app = initialize_app(options, Some(settings)).await?;
    let service = get_data_connect_service(Some(app.clone()), Some("https://example/graphql"))
        .await?;

    let mut variables = BTreeMap::new();
    variables.insert("id".into(), serde_json::json!(123));

    let response = service
        .execute(QueryRequest {
            operation: "query GetItem($id: ID!) { item(id: $id) { id } }".into(),
            variables,
        })
        .await?;

    println!("response payload: {}", response.data);

    Ok(())
}
```

## Current Functionality

- **Component wiring** – `register_data_connect_component` registers a `data-connect` component allowing apps to
  asynchronously retrieve a `DataConnectService` via `get_data_connect_service`.
- **Async service stub** – `DataConnectService::execute` is async and returns a synthetic `QueryResponse` that echoes the
  request and selected endpoint, making it safe to call from wasm and native event loops.
- **Endpoint handling** – Supports per-endpoint instances via instance identifiers/options.
- **Errors/constants** – Basic error codes (`data-connect/invalid-argument`, `data-connect/internal`) and component name
  constant.
- **Tests** – Async unit tests covering successful execution and empty-operation validation.

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
