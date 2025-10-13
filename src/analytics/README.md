# Firebase Analytics Port (Rust)

This directory contains the initial Rust scaffolding for the Firebase Analytics SDK. The objective is to recreate the
modular `@firebase/analytics` experience while integrating with the shared component system used across the crate.

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
