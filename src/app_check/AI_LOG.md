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


## 20 October 2025

> PROMPT: Do the same analysis for the app_check module (previous prompt was: Now compare the original JS/Typescript files in ./packages/app and the ported files in Rust in ./src/app, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust)

Explored
  └ List app-check
    List app_check
    Read README.md, api.rs, state.rs, providers.rs, types.rs, errors.rs, interop.rs, token_provider.rs, mod.rs,
logger.rs,
         api.ts, internal-api.ts, providers.ts
    Search emulator in app_check

> Thinking
  I'm estimating the Rust implementation covers about 25-30% of the JavaScript App Check API, with many core parts
  missing like storage, proactive refresh, and debug support. I'm breaking down major areas to check coverage,
  especially around token handling, providers, and internal bridging, while planning to verify component registration
  details next.

• Explored
  └ Search register in app_check

  Evaluating feature coverage and gaps (1m 20s • Esc to interrupt)

  Estimated coverage ≈20 % of the JS App Check SDK.

  - Implemented pieces cover only the happy-path API surface: initialize_app_check, token fetch, limited-use fetch,
  listener wiring, and manual auto-refresh toggling (src/app_check/api.rs:15, src/app_check/state.rs:37). Custom
  providers work for synchronous token callbacks (src/app_check/providers.rs:8), and there’s an internal bridge so other
  modules (e.g. Firestore) can treat App Check as a token provider (src/app_check/token_provider.rs:9, src/app_check/
  interop.rs:1).
  - All platform integrations that dominate the JS SDK are missing. There’s no component registration or service
  factory equivalent to the JS wiring in packages/app-check/src/factory.ts:33 and _registerComponent calls, so other
  modules cannot obtain App Check via the component system. Tokens only live in memory; the persistence layer, IndexedDB
  helpers, and debug-token storage (packages/app-check/src/storage.ts:32, packages/app-check/src/indexeddb.ts:1,
  packages/app-check/src/debug.ts:1) have no Rust counterpart.
  - Proactive refresh, throttling, and visibility-aware backoff are absent: the scheduler and retry policy in packages/
  app-check/src/proactive-refresh.ts:26 and the state-machine logic in packages/app-check/src/internal-api.ts:61 aren’t
  ported, leaving set_token_auto_refresh_enabled as a no-op flag (src/app_check/state.rs:98).
  - Provider support is skeletal. ReCAPTCHA providers immediately return “not implemented” errors (src/app_check/
  providers.rs:77, src/app_check/providers.rs:100), while the JS versions perform token acquisition, throttling, and
  heartbeat integration (packages/app-check/src/providers.ts:33). Debug-mode, emulator support, heartbeat usage, and
  observer error channels defined across packages/app-check/src/api.ts:34, packages/app-check/src/client.ts:1, and
  packages/app-check/src/util.ts:1 are missing entirely.

  Given that only the minimal API shell and custom-provider plumbing exist while the JS module’s storage, refresh
  lifecycle, provider implementations, debug/emulator flows, and component factories are unported, a 20 % completion
  estimate is a reasonable upper bound.