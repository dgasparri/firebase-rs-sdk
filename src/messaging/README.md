# Firebase Messaging

Rust implementation of the Firebase Cloud Messaging (FCM) web SDK surface. The goal is to mirror the `@firebase/messaging` APIs so applications can manage web push permissions, registration tokens and foreground/background notifications from Rust (including WASM builds).

Porting status: 40% `[####      ]` ([details](https://github.com/dgasparri/firebase-rs-sdk/blob/main/src/messaging/PORTING_STATUS.md))

## Quick Start Example

```rust,no_run
use firebase_rs_sdk::messaging::{
    self, PermissionState, PushSubscriptionManager, ServiceWorkerManager,
};

#[cfg(all(target_arch = "wasm32", feature = "wasm-web", feature = "experimental-indexed-db"))]
// Requires building with `--features wasm-web,experimental-indexed-db`.
async fn initialise_messaging() -> messaging::MessagingResult<()> {
    if !messaging::is_supported() {
        return Err(messaging::unsupported_browser(
            "Browser is missing push APIs",
        ));
    }

    let mut sw_manager = ServiceWorkerManager::new();
    let registration = sw_manager.register_default().await?;

    let mut push_manager = PushSubscriptionManager::new();
    let vapid_key = "<your-public-vapid-key>";

    let messaging = messaging::get_messaging(None).await?;
    if matches!(messaging.request_permission().await?, PermissionState::Granted) {
        let subscription = push_manager.subscribe(&registration, vapid_key).await?;
        let details = subscription.details()?;
        let _token = messaging.get_token(Some(vapid_key)).await?;
    }
    Ok(())
}
```

## References to the Firebase JS SDK

- QuickStart: <https://firebase.google.com/docs/cloud-messaging/get-started?platform=web>
- API: <https://firebase.google.com/docs/reference/js/messaging.md#messaging_package>
- Github Repo - Module: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/messaging>
- Github Repo - API: <https://github.com/firebase/firebase-js-sdk/tree/main/packages/firebase/messaging>
