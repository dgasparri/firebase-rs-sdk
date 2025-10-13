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