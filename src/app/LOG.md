Thinking
  There is a lot to cover here, so I'll list the key missing modules and incomplete features in the Rust port compared to the JS version. I'll highlight gaps like the heartbeat and platform logger
  services, missing server app support, incomplete register_version hooking, environment detection stubs, and asynchronous app deletion differences. I'll also call out missing tests and some smaller
  but important behavioral differences around FirebaseApp checks and normalization. This will help clarify what remains to import or implement to reach parity.

> - Heartbeat & IndexedDB stack missing – None of the JS heartbeat pieces (packages/app/src/heartbeatService.ts, indexeddb.ts, associated tests) exist in Rust. registry::get_provider already probes a
  heartbeat provider, but no HeartbeatService component or storage layer has been ported, so installs/messaging can’t emit heartbeat headers.
  - Platform logger service absent – The JS platformLoggerService.ts component isn’t mirrored. Although the PlatformLoggerService trait exists, there is no implementation or component registration
  (registerCoreComponents) to populate version metadata, so other SDKs can’t build the Firebase user‑agent string.
  - Core component registration not wired – JS registerCoreComponents registers platform logger, heartbeat, and calls registerVersion. The Rust module never auto-registers these components and
  register_version only updates an internal HashMap instead of exposing a ComponentType::VERSION component. Version metadata therefore isn’t discoverable by other services.
  - Server app support unimplemented – initialize_server_app simply returns InvalidServerAppEnvironment, and FirebaseServerApp lacks the ref-count/finalization logic, token TTL validation, and
  version registration from firebaseServerApp.ts. _isFirebaseServerApp* helpers also aren’t exposed.
  - Environment & default config helpers stubbed – get_default_app_config, is_browser, and is_web_worker all return placeholders, so default apps cannot auto-initialize from the usual Firebase config
  sources and browser/server gating doesn’t match JS behaviour.
  - Internal API gaps – Functions like _clearComponents, _removeServiceInstance, and _isFirebaseServerApp from internal.ts aren’t surfaced. The existing register_component/add_component mirrors some
  behaviour, but there’s no way to clear global components for tests, and heartbeat triggering in _getProvider is still a TODO.
  - App component registration incomplete – During initialize_app, the 'app' component is registered with a factory returning Arc<()> instead of the FirebaseApp, so consumers resolving the 'app'
  provider won’t get the expected instance.
  - Server-side options & equality helpers simplified – Rust’s deep_equal_options/config are simple == checks, missing JS’s deep comparison nuances (e.g., handling undefined fields and measurement ID
  defaults). FirebaseApp getters don’t guard against deleted apps, unlike JS’s checkDestroyed.
  - Testing parity missing – None of the JS test suites (api.test.ts, firebaseApp.test.ts, firebaseServerApp.test.ts, internal.test.ts, heartbeatService.test.ts, indexeddb.test.ts,
  platformLoggerService.test.ts) have Rust equivalents, leaving core behaviour (duplicate detection, server-app reference counting, heartbeat storage, component updates) uncovered.

  Overall, to finish the import you’ll need to port the heartbeat/platform-logger components (with IndexedDB persistence), implement server-app creation + ref counting, support default-config/
  bootstrap helpers, expose the remaining internal APIs, fix the 'app' component factory, and translate the JS tests so the Rust port matches @firebase/app semantics.

▌ what is the heartbeat?

> In @firebase/app, the “heartbeat” is a small telemetry service that tracks when the SDK last made network requests so other SDKs (Installations, Auth, Messaging, etc.) can attach a x-firebase-
  client header (base64 JSON of recent “heartbeats”) to backend calls. The JS implementation:

  - registers a private heartbeat component during registerCoreComponents.
  - persists a rolling window of heartbeat timestamps per user agent in IndexedDB (via indexeddb.ts).
  - exposes two methods: triggerHeartbeat() (record “today” if not already stored) and getHeartbeatsHeader() (return base64 heartbeat payload and prune/persist cache).
  - the App “internal” API retrieves the heartbeat provider before serving another component; if the service exists it runs triggerHeartbeat() so the next outbound request can include an up‑to‑date
  header.

  In Rust, that whole slice is currently missing—the registry only has a TODO when a 'heartbeat' provider is present. Porting it would mean creating a HeartbeatService implementation (with a storage
  backend; for WASM builds likely using web-sys IndexedDB, native maybe in-memory/SQLite), registering it as a private component alongside the platform logger, and exposing methods so other services
  can request the header when sending requests.


Updated Plan
  └ □ Add core infrastructure: heartbeat service/storage, platform logger component, and register core components invoked at module init.
    □ Implement server app parity (initialize_server_app logic, FirebaseServerApp ref counting, helper API) and align internal APIs (add_component, clear, etc.).
    □ Enhance option/environment helpers and port representative tests with new mock infrastructure.

  Considering task planning (17s • Esc to interrupt)

- Added core infrastructure for Firebase App: new platform-logger and heartbeat components (with shared in-memory storage) now auto-register on app initialization, enabling version reporting and
  heartbeat triggering just like the JS SDK (src/app/core_components.rs, src/app/heartbeat.rs, src/app/platform_logger.rs, src/app/api.rs, src/app/registry.rs, src/app/types.rs, src/app/mod.rs).
  - Updated register_version to emit real VERSION components so the platform logger can gather library metadata, and fixed the 'app' provider to return the actual FirebaseApp instance.
  - Refreshed src/app/README.md with current status, detailed next steps, and a progress note describing the new services.

  Tests: cargo test --lib app::