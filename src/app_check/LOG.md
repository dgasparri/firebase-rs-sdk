## 12 October 2025

 App Check Integration Review

  - src/app_check/api.rs:15 still instantiates services outside the component system. In the
  JS SDK app check registers 'app-check'/'app-check-internal' components so _getProvider(app,
  'app-check') returns a cached instance. Today the Rust port ignores the app container
  entirely, so other Firebase modules cannot fetch App Check via ComponentContainer. You’ll
  want to add component definitions (using the new component module) and register them through
  registry::register_component.
  - src/app_check/types.rs:134 models AppCheck as a lightweight wrapper without _delete, heartbeat
  dependency injection, or the activate() flow from app-types. The JS AppCheckService keeps a
  heartbeat provider and integrates with the token refresher in internal-api.ts; adding that
  structure would let the service participate in app shutdown and auto-refresh logic.
  - src/app_check/types.rs:8 defines APP_CHECK_COMPONENT_NAME, but we never register a component
  under that name nor the internal variant. Without registering both the public and internal
  components, FirebaseAppCheckInternal consumers cannot obtain the internal bridge from the
  container.
  - src/app_check/interop.rs:1 now provides FirebaseAppCheckInternal, but nothing connects it
  to the app container. After you define the internal component, its instance factory should
  construct this struct so _getProvider(app, 'app-check-internal') works.
  - src/app/namespace.rs:1 exposes only core App APIs. The JS namespace adds firebase.appCheck()
  and firebase.appCheck().activate(...); wiring similar entry points (built atop the component
  provider) would mirror the app-types contract.

  Once those pieces are connected—especially registering the components and moving initialization
  through the provider—you’ll have parity with the TS integration and other modules can depend on
  App Check via the shared component/namespace surface.


    - Added App Check interop surface mirroring the TS types (src/app_check/interop.rs:1):
  FirebaseAppCheckInternal now provides token getters, limited-use tokens, and listener management
  with proper bridging into the existing App Check state. Tests cover token retrieval and listener
  lifecycle.
  - Extended the App Check type layer (src/app_check/types.rs:10) with internal component
  constants, token listener aliases, cloned token results, and convenience methods on AppCheck for
  token access/observation that leverage the util observer helpers (src/util/subscribe.rs:1, re-
  exported from src/util/mod.rs:1).
  - Surfaced the new interop exports via src/app_check/mod.rs:6, and re-exported FirebaseAppConfig
  in src/app/mod.rs:10 so supporting code (including the new tests) can construct minimal apps.
  The app Private/API modules were untouched.
