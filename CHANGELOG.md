# Changelog

## v1.34.1

- messaging: add multi-tab coordination for service worker registration and token refresh to avoid duplicate registrations/FCM calls, plus cross-tab waits when another tab is refreshing.
- messaging: bump module porting status to 45% in docs/status to reflect the new cross-tab work.
- tests: serialize component registration/mocking in functions/analytics tests to prevent cross-test races under multithreaded runs.
- analytics: add background initialization that resolves dynamic config on creation and seeds the gtag snapshot with measurement IDs and collection overrides.
- analytics: implement robust config fetching with retry/backoff, timeout fallback to local measurement IDs, and deterministic mocks so tests run safely in parallel.
- analytics: mirror `setAnalyticsCollectionEnabled` by propagating the toggle to the gtag `ga-disable-<id>` state while still gating transport dispatch.
