## Porting status

- analytics 30% `[###        ]`

==As of May 29th, 2026==

Prompt: Compare the original JS/Typescript files in ./packages/analytics and the ported files in Rust in ./src/analytics, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

The analytics port is about 30 % complete. We now mirror the JS initialization flow: dynamic config is fetched with retry/backoff, falls back to local measurement IDs, and runs in the background so gtag state is populated automatically. Collection toggles now surface the JS `setAnalyticsCollectionEnabled` behavior by updating the gtag bootstrap state alongside the local dispatcher switch. What’s in place:

  - Component wiring, in-memory event capture, and a Measurement Protocol dispatcher that works with GA4 when
  credentials are provided (src/analytics/api.rs:33, src/analytics/transport.rs:1).
  - Dynamic config fetching with retry/backoff, timeout handling, and fallback to local measurement IDs, plus background
  initialization of gtag state (src/analytics/config.rs:1, src/analytics/api.rs:246).
  - Default event parameters, consent/settings caching, collection enable/disable toggles that also set the
  `ga-disable-<id>` state, and a gtag bootstrap snapshot for WASM consumers (src/analytics/api.rs:90-222,
  src/analytics/gtag.rs:1).
  - Unit coverage around initialization, gtag state, config fetch fallbacks, and Measurement Protocol dispatch.

Large pieces are still missing—full gtag initialization (script injection, FID wiring), consent persistence, user identity helpers, recommended event wrappers, debug tooling, and the full test suite—so there’s still plenty to port.


## Implemented

- Component registration so `get_analytics` and `register_analytics_component` mirror the JS modular API.
- In-memory event recording through `Analytics::log_event`, useful for testing and consumers that inspect sent payloads.
- Measurement Protocol dispatcher that can forward events to GA4 when supplied with credentials, plus convenience
  helpers to derive the measurement ID from Firebase app options or the Firebase config endpoint.
- Default event parameter handling, consent/settings storage, collection toggles that mirror `setAnalyticsCollectionEnabled`,
  and a shared gtag bootstrap state so WASM consumers can initialise scripts with consistent defaults before sending events.
- Dynamic config fetching (`/v1alpha/projects/-/apps/{app-id}/webConfig`) with retry/backoff, timeout handling, and
  fallback to local measurement IDs so initialization can proceed offline.
- Basic error handling (`analytics/invalid-argument`, `analytics/internal`, `analytics/network`,
  `analytics/config-fetch-failed`, `analytics/missing-measurement-id`) and unit coverage for component wiring, config
  resolution, and optional network dispatch.

## Still to do

- Gtag bootstrap & script handling: port the remaining `initialize-analytics.ts` bits (gtag wrapping, script injection,
  FID propagation, and configurable global names) so WASM consumers can mirror the JS lifecycle.
- Analytics settings & consent: add consent default persistence, cookie/extension warnings, and align settings merging
  semantics with the JS SDK.
- User properties and helpers: implement `setUserId`, `setUserProperties`, screen tracking, and the collection of
  helper wrappers found in the JS functions module.
- Automatic data collection & debug tooling: add debug view toggles, session state management, and automatic lifecycle
  events.
- Platform-specific behaviours, logging utilities, and the comprehensive test suite that exercises the behaviours
  above.

## Next Steps - Detailed Completion Plan

1. **Gtag Initialization & Script Wiring**
   - Finish porting `initialize-analytics.ts` by wrapping gtag, handling custom `gtag`/`dataLayer` names, and injecting
     the GA tag in WASM builds when requested.
   - Thread FID and deferred consent defaults through the bootstrap so the first `config` call matches the JS SDK.
2. **Analytics Settings & Consent Controls**
   - Persist consent defaults, mirror cookie/extension warnings, and align settings merging with the JS SDK semantics.
   - Add storage/backfill for user-provided defaults so late initialization can apply them automatically.
3. **User Identity & Helper APIs**
   - Implement `setUserId`, `setUserProperties`, `setCurrentScreen`, and the recommended event helpers so downstream
     modules can rely on the typed wrappers provided by the JS SDK.
