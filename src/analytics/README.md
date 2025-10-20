# Firebase Analytics Module

## Introduction

The Analytics module ports the modular `@firebase/analytics` SDK to Rust. It wires into the shared Firebase component
system so other services can obtain an `Analytics` instance that records events and optionally forwards them to Google
Analytics using the GA4 Measurement Protocol.


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



## Quick Start Example

```rust
use std::collections::BTreeMap;

use firebase_rs_sdk_unofficial::analytics::{
    get_analytics, MeasurementProtocolConfig, MeasurementProtocolEndpoint,
};
use firebase_rs_sdk_unofficial::app::{initialize_app, FirebaseAppSettings, FirebaseOptions};

fn main() -> firebase_rs_sdk_unofficial::analytics::AnalyticsResult<()> {
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
- Measurement Protocol dispatcher that can forward events to GA4 when supplied with a measurement ID and API secret.
- Basic error handling (`analytics/invalid-argument`, `analytics/internal`, `analytics/network`) and unit coverage for
  component wiring and network dispatch.

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

1. **Initialization & Config Fetch**
   - Port the logic from `packages/analytics/src/initialize-analytics.ts` to derive `AnalyticsSettings`, register the
     gtag environment, and resolve the measurement ID / app ID pair automatically from the app options or remote config.
   - Implement the `get-config.ts` behaviour to call the Firebase config endpoint, store the response, and hydrate the
     measurement dispatcher without requiring manual configuration.
2. **Analytics Settings & Consent Controls**
   - Add Rust equivalents for `setAnalyticsCollectionEnabled`, `_setConsentDefaultForInit`, and
     `setDefaultEventParameters`, including basic persistence of consent state and the ability to pause event delivery.
3. **User Identity & Helper APIs**
   - Implement `setUserId`, `setUserProperties`, `setCurrentScreen`, and the recommended event helpers so downstream
     modules can rely on the typed wrappers provided by the JS SDK.
