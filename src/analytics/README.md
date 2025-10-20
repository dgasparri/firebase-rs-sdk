# Firebase Analytics Port (Rust)

This directory contains the initial Rust scaffolding for the Firebase Analytics SDK. The objective is to recreate the
modular `@firebase/analytics` experience while integrating with the shared component system used across the crate.


## Porting status

- analytics 5% \[#          \]

==As of October 20th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/analytics and the ported files in Rust in ./src/analytics, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

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


## Current Functionality

- **Component wiring** – `register_analytics_component` registers the public `analytics` component so apps can obtain an
  instance via `get_analytics`.
- **Analytics service stub** – Minimal `Analytics` struct with `log_event` that validates the event name and records
  events in-memory for testing purposes.
- **Errors & constants** – Basic error codes (`analytics/invalid-argument`, `analytics/internal`) and component name
  constant.
- **Tests** – Unit test covering component retrieval and event recording.

This is sufficient for other modules to depend on the analytics provider for structural integration, but it lacks real
measurement collection, transport, and configuration management.

## Work Remaining (vs `packages/analytics`)

1. **Measurement protocol integration**
   - Implement the network layer that sends events to Google Analytics (GA4) endpoints, handling API key/app ID.
2. **Initialization & config fetch**
   - Port `initialize-analytics.ts` and `get-config.ts` behaviour (automatic app measurement ID, remote config fetch).
3. **Analytics settings & consent**
   - Implement `setAnalyticsCollectionEnabled`, consent state management, and persistence.
4. **Event helpers**
   - Mirror helper functions for screen tracking, app lifecycle events, and recommended event wrappers.
5. **User properties & ID**
   - Add `setUserId`, `setUserProperties`, and related APIs with persistence and validation.
6. **Automatic data collection / debug mode**
   - Support debug view toggles, session IDs, and automatic event logging based on environment.
7. **Platform-specific features**
   - Port browser-only logic (session storage, visibility listeners) and account for React Native/Cordova differences.
8. **Logger & helper utilities**
   - Mirror JS `logger.ts`, helper validation utilities, and metadata constants.
9. **Testing parity**
   - Translate the JS test suite covering API behaviour, config fetch, and helper utilities.

Completing these tasks will move the Rust Analytics module from a stub into a functional analytics client aligned with the
JavaScript SDK.
