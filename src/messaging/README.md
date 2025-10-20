# Firebase Messaging Port (Rust)

This directory hosts the early-stage Rust port of the Firebase Cloud Messaging (FCM) web SDK. The aim is to reproduce
`@firebase/messaging` so apps can request notification permissions, obtain registration tokens, and handle messages.

## Porting status

- messaging 3% \[          \]

==As of October 20th, 2025== 

Prompt: Compare the original JS/Typescript files in ./packages/messaging and the ported files in Rust in ./src/messaging, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust

I'm gathering key points about the messaging module's features and gaps—like the minimal Rust stub missing push and service worker support, its limited API implementations, error coverage around 3%, and highlighting relevant JS files and paths—so I can create a clear and thorough final summary for the user.

Coverage is roughly 3 % of the JS Messaging SDK.

  - Rust currently wires a messaging component and exposes request_permission, get_token, and delete_token, but
  everything is in-memory: tokens are random strings keyed by app name, permission always returns true, and deletion
  just clears the map (src/messaging/api.rs:1). Error enums mirror only three codes (src/messaging/error.rs:1). There’s
  no service-worker, push-subscription, or network interaction.
  - The JS module is vast. packages/messaging/src/messaging-service.ts:1 sets up dependencies on Installations,
  Analytics, service workers, BigQuery logging, and background handlers; token APIs drive real browser flows, notice
  packages/messaging/src/api/getToken.ts:1 prompting notification permission, updating VAPID keys, and delegating
  to the token manager. The token manager itself handles IndexedDB persistence, FCM REST calls, weekly refresh, push
  subscription lifecycle, and backend delete/update calls (packages/messaging/src/internals/token-manager.ts:1).
  Surrounding helpers manage service worker registration, environment guards, logging, background/foreground message
  listeners, isSupported checks, delivery metrics export, and testing utilities spread across packages/messaging/src/
  helpers/*, packages/messaging/src/listeners/*, packages/messaging/src/util/*, etc.

Given the Rust port only reproduces the API shape with a trivial stub while omitting browser integration, Installations/Analytics/App Check interop, network requests, persistence, subscription management, message dispatch, and emulator/testing support, its functionality today is only a few percent of what the JavaScript SDK provides.


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
