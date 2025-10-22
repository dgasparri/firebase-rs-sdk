# Firebase Analytics Module

## Introduction

The Analytics module ports the modular `@firebase/analytics` SDK to Rust. It wires into the shared Firebase component
system so other services can obtain an `Analytics` instance that records events and optionally forwards them to Google
Analytics using the GA4 Measurement Protocol.


## Porting status

- analytics 20% `[##         ]`

==As of October 20th, 2025==

Prompt: Compare the original JS/Typescript files in ./packages/analytics and the ported files in Rust in ./src/analytics, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

I’d put the analytics port at roughly 20 % complete now. We still only cover a slice of the JS SDK, but it’s notably beyond the original 5 % scaffold. What’s in place:

  - Component wiring, in-memory event capture, and a Measurement Protocol dispatcher that works with GA4 when
  credentials are provided (src/analytics/api.rs:33, src/analytics/transport.rs:1).
  - Automatic measurement resolution via Firebase app options or the /webConfig endpoint (src/analytics/config.rs:1,
  src/analytics/api.rs:118).
  - Default event parameters, consent/settings caching, collection enable/disable toggles, and a gtag bootstrap state
  for WASM consumers (src/analytics/api.rs:90-213, src/analytics/gtag.rs:1).
  - Unit coverage around those behaviours, plus opt-in Measurement Protocol tests.

Large pieces are still missing—full gtag initialization, consent persistence, user identity helpers, recommended event wrappers, debug tooling, and the full test suite—so there’s plenty left, but the module is no longer just a stub.


## Quick Start Example

```rust,no_run
use std::collections::BTreeMap;

use firebase_rs_sdk::analytics::{
    get_analytics, MeasurementProtocolConfig, MeasurementProtocolEndpoint,
};
use firebase_rs_sdk::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = FirebaseOptions {
        api_key: Some("AIza...".into()),
        app_id: Some("1:1234567890:web:abcdef".into()),
        measurement_id: Some("G-1234567890".into()),
        ..Default::default()
    };
    let settings = FirebaseAppSettings {
        name: Some("analytics-demo".into()),
        ..Default::default()
    };

    let app = initialize_app(options, Some(settings))?;
    let analytics = get_analytics(Some(app))?;

    // Provide the GA4 measurement ID and API secret generated in Google Analytics.
    let config = MeasurementProtocolConfig::new("G-1234567890", "api-secret")
        .with_endpoint(MeasurementProtocolEndpoint::Collect);
    analytics.configure_measurement_protocol(config)?;

    let mut params = BTreeMap::new();
    params.insert("engagement_time_msec".to_string(), "100".to_string());
    analytics.log_event("tutorial_begin", params)?;

    Ok(())
}
```

## Implemented

- Component registration so `get_analytics` and `register_analytics_component` mirror the JS modular API.
- In-memory event recording through `Analytics::log_event`, useful for testing and consumers that inspect sent payloads.
- Measurement Protocol dispatcher that can forward events to GA4 when supplied with credentials, plus convenience
  helpers to derive the measurement ID from Firebase app options or the Firebase config endpoint.
- Default event parameter handling, consent/settings storage, collection toggles, and a shared gtag bootstrap state so
  WASM consumers can initialise scripts with consistent defaults before sending events.
- Dynamic config fetching (`/v1alpha/projects/-/apps/{app-id}/webConfig`) with caching so the measurement ID can be
  resolved automatically before wiring the dispatcher.
- Basic error handling (`analytics/invalid-argument`, `analytics/internal`, `analytics/network`,
  `analytics/config-fetch-failed`, `analytics/missing-measurement-id`) and unit coverage for component wiring, config
  resolution, and optional network dispatch.

## Still to do

- Initialization & config fetch: port `initialize-analytics.ts` and `get-config.ts` to derive measurement IDs,
  app settings, and remote configuration automatically.
- Analytics settings & consent: mirror `setAnalyticsCollectionEnabled`, consent defaults, and persistence semantics.
- User properties and helpers: implement `setUserId`, `setUserProperties`, screen tracking, and the collection of
  helper wrappers found in the JS functions module.
- Automatic data collection & debug tooling: add debug view toggles, session state management, and automatic lifecycle
  events.
- Platform-specific behaviours, logging utilities, and the comprehensive test suite that exercises the behaviours
  above.

## Next Steps - Detailed Completion Plan

1. **Gtag Initialization & Settings**
   - Finish porting `initialize-analytics.ts` by wiring the gtag environment, consent defaults, and automatic
     configuration properties using the resolved measurement ID.
   - Add gtag bootstrapper (script injection hooks, js initialisation) and surface the analytics settings configuration
     points (`config`, send_page_view toggles, etc.).
2. **Analytics Settings & Consent Controls**
   - Add Rust equivalents for `setAnalyticsCollectionEnabled`, `_setConsentDefaultForInit`, and
     `setDefaultEventParameters`, including basic persistence of consent state and the ability to pause event delivery.
3. **User Identity & Helper APIs**
   - Implement `setUserId`, `setUserProperties`, `setCurrentScreen`, and the recommended event helpers so downstream
     modules can rely on the typed wrappers provided by the JS SDK.
