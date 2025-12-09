# Changelog

## 2026-05-29

- analytics: add background initialization that resolves dynamic config on creation and seeds the gtag snapshot with measurement IDs and collection overrides.
- analytics: implement robust config fetching with retry/backoff, timeout fallback to local measurement IDs, and deterministic mocks so tests run safely in parallel.
- analytics: mirror `setAnalyticsCollectionEnabled` by propagating the toggle to the gtag `ga-disable-<id>` state while still gating transport dispatch.
