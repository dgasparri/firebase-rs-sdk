# Firebase Messaging Port (Rust)

This directory hosts the early-stage Rust port of the Firebase Cloud Messaging (FCM) web SDK. The aim is to reproduce
`@firebase/messaging` so apps can request notification permissions, obtain registration tokens, and handle messages.

## Current Functionality

- **Component wiring** – `register_messaging_component` registers the `messaging` component so apps can call
  `get_messaging` via the shared component container.
- **Messaging service stub** – In-memory token generation that supports `request_permission`, `get_token`, and
  `delete_token` (with forced regeneration on delete).
- **Errors/constants** – Basic error codes (`messaging/invalid-argument`, `messaging/internal`,
  `messaging/token-deletion-failed`) and component name constant.
- **Tests** – Unit test validating token stability and refresh semantics.

The stub enables structural integration but does not interact with the real FCM infrastructure or browser APIs.

## Work Remaining (vs `packages/messaging`)

1. **Browser permission & service worker integration**
   - Implement actual notification permission prompts, service worker registration, and push subscription handling.
2. **Installation / FCM token fetch**
   - Exchange with the Firebase Installations service and FCM backend to obtain registration tokens (REST calls, VAPID
     keys, heartbeat headers).
3. **Token persistence & multi-tab coordination**
   - Store tokens in IndexedDB/localStorage, handle tab focus events, and support token refresh intervals.
4. **Foreground/background message handling**
   - Port message event dispatching, background handler registration, and payload decoding.
5. **Public API parity**
   - Implement `onMessage`, `getToken`, `deleteToken`, `isSupported`, `experimental` helpers, etc., per the JS SDK.
6. **Platform/environment guards**
   - Mirror checks for browser support (service worker availability, notifications API) and React Native/Cordova
     behaviour.
7. **Testing & emulator support**
   - Port JS tests (token refresh, message handling, permission flows) and add emulator integration if applicable.

Addressing these items will bring the Rust Messaging module to parity with the JavaScript SDK and enable real FCM usage.
