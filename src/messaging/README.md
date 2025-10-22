# Firebase Messaging Port (Rust)

## Introduction

Rust implementation of the Firebase Cloud Messaging (FCM) web SDK surface. The goal is to mirror the `@firebase/messaging` APIs so applications can manage web push permissions, registration tokens and foreground/background notifications from Rust (including WASM builds).

## Porting status

- messaging 22% `[=====     ]`

## Quick Start Example

```rust,no_run
use firebase_rs_sdk_unofficial::messaging::{
    self, PermissionState, PushSubscriptionManager, ServiceWorkerManager,
};

#[cfg(all(target_arch = "wasm32", feature = "wasm-web"))]
async fn initialise_messaging() -> messaging::error::MessagingResult<()> {
    if !messaging::is_supported() {
        return Err(messaging::error::unsupported_browser(
            "Browser is missing push APIs",
        ));
    }

    let mut sw_manager = ServiceWorkerManager::new();
    let registration = sw_manager.register_default().await?;

    let mut push_manager = PushSubscriptionManager::new();
    let vapid_key = "<your-public-vapid-key>";

    let messaging = messaging::get_messaging(None)?;
    if matches!(messaging.request_permission().await?, PermissionState::Granted) {
        let subscription = push_manager.subscribe(&registration, vapid_key).await?;
        let details = subscription.details()?;
        let _token = messaging.get_token(Some(vapid_key)).await?;
    }
    Ok(())
}
```

## Implemented

- Component registration that exposes `Messaging` through the shared component system.
- In-memory token management for `get_token` and `delete_token` with validation of VAPID keys.
- `messaging::is_supported()` guard that mirrors `packages/messaging/src/api/isSupported.ts` for web builds.
- Asynchronous `Messaging::request_permission` flow that awaits `Notification.requestPermission` and surfaces
  `messaging/permission-blocked` and `messaging/unsupported-browser` errors.
- `ServiceWorkerManager` helper that registers the default Firebase messaging worker (mirrors
  `helpers/registerDefaultSw.ts`) and caches the resulting registration for reuse.
- WASM-only `PushSubscriptionManager` that wraps `PushManager.subscribe`, returns subscription details and supports
  unsubscribe/error mapping aligned with the JS SDK implementation.
- Token persistence layer that stores token metadata (including subscription details and creation time) per app,
  mirroring the IndexedDB-backed token manager with localStorage/native fallbacks.
- Minimal error types for invalid arguments, internal failures and token deletion.
- Non-WASM unit test coverage for permission/token flows, invalid arguments, token deletion, service worker and push
  subscription stubs.

## Still to do

- Track permission changes across sessions and expose notification status helpers similar to the JS SDK.
- Call the Installations and FCM REST endpoints to create, refresh and delete tokens, including weekly refresh checks.
- Coordinate multi-tab state and periodic refresh triggers using IndexedDB change listeners (BroadcastChannel / storage events).
- Foreground/background message listeners, payload decoding and background handlers.
- Environment-specific guards (SW vs window scope), emulator/testing helpers and extended error coverage.

## Next steps - Detailed completion plan

1. Implement the Installations/FCM REST interactions for token create/update/delete, mirroring `internals/token-manager.ts`.
2. Add multi-tab coordination (BroadcastChannel/localStorage) so IndexedDB state stays in sync across contexts.
3. Port message delivery APIs (`onMessage`, `onBackgroundMessage`) and event dispatchers, including WASM gating for background handlers.
4. Expand the error catalog to match `packages/messaging/src/util/errors.ts`, update documentation and backfill tests for the newly added behaviours.
