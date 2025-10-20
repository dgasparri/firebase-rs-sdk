# 12 October 2025

Updated Plan
  └ ✔ Lay down analytics module structure (constants, error placeholders, mod wiring).
    ✔ Implement minimal Analytics service with component registration and get_analytics/log_event stubs.
    ✔ Expose API via crate root and add lightweight tests; format and run suite.

> Brought in a lightweight Analytics module so other services can resolve an analytics instance and log events.

  - Added src/analytics with constants, error type, and an Analytics service that registers via the component system,
  responds to get_analytics, and stores logged events (basic validation + in-memory capture).
  - Exposed the module at the crate root so users can call firebase-rs-sdk-unofficial-porting::analytics::*.
  - Included a unit test that initializes an app, retrieves analytics, and verifies log_event records entries.

  Formatting and tests: cargo fmt, cargo test.


  analytics in the Javascript SDK depens on installation. Now that you have done a minimal API for installation, do we
▌ need to make changes to analytics?

> For the in‑memory analytics stub we just added, no immediate changes are required—events are captured locally and
  don’t leave the process, so the module doesn’t yet need an Installation ID.

  That said, the real JS SDK always obtains a Firebase Installation ID (FID) via getInstallations(). Once we move beyond
  the stub—e.g., start sending events to GA4, implement config fetch, or support debug view—we’ll need to:

  - resolve the installations component from analytics initialization;
  - cache the FID and refresh auth tokens as needed when building Measurement Protocol requests.

  So: no action required right now, but installations integration should definitely be part of the future analytics
  roadmap when we implement the real transport/config layers.


  # 20 October 2025

  > PROMPT: Compare the original JS/Typescript files in ./packages/{module} and the ported files in Rust in ./src/{module}, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

  I'll estimate that roughly 5% of the Firebase JS SDK analytics features have been ported to Rust, focusing mainly
  on foundational pieces like component registration and minimal log_event stubs. The vast majority—including
  initialization, configuration, gtag integration, and advanced features like setUserId—remain unported. I'll break
  down which features exist versus which are missing, referencing specific source files in both packages/analytics for
  TypeScript and src/analytics for Rust to support this overview.

> Estimated coverage is ~5% of the JS analytics SDK. The Rust side currently wires up the component and records events
  in memory (src/analytics/api.rs:1 plus src/analytics/error.rs:1, src/analytics/constants.rs:1), but everything else is
  still a scaffold.

  Key JS features that remain unported span almost every module: the full public API surface in packages/analytics/src/
  api.ts:1 (initialization pathways, isSupported, consent, user properties, default params, dozens of event helpers),
  the gtag integration and environment guards in packages/analytics/src/factory.ts:1, configuration fetching in
  packages/analytics/src/initialize-analytics.ts:1 and packages/analytics/src/get-config.ts:1, validation/utilities in
  packages/analytics/src/functions.ts:1 and packages/analytics/src/helpers.ts:1, logging and type definitions, plus the
  comprehensive test suite. None of these have Rust counterparts yet.

  Given that only the minimal component plumbing and an in-memory log_event stub are present, while the rest of the
  measurement, configuration, consent, user state, platform hooks, and testing infrastructure are absent, 5% is a
  reasonable upper-bound estimate.