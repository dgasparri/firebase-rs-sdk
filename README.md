
https://github.com/firebase/firebase-js-sdk
https://firebase.google.com/docs/web/setup
https://firebase.google.com/docs/reference/js/


This directory hosts the early-stage Rust port of the Firebase Firestore SDK. The goal is to mirror the JavaScript
implementation while exposing idiomatic Rust APIs that interoperate with the shared component/app infrastructure already
present in this repository.

read and follow the insturctions in ./AGENTS.md . In the last session we were working on the storage module, and you
▌ suggested to "Implement Auth/App Check token retrieval and attach headers during request execution." Do it

## Todo

1. Implement the tests from ./packages/app
2. Implement the tests from ./packages/app-check
3. ./packages/auth is only partially ported. Check also ./src/auth/README.md
4. document functions
5. see auth LOG.md (You can keep it ergonomic by gating the web adapters behind a Cargo feature (wasm-web) ())
6. CONTRIBUTING.md in the JS SDK and API documentation https://chatgpt.com/c/68eccf4b-d1c8-8328-845f-d39a4472284d

Improve documentation of public API comparing it to the original library

rustdoc
/// for item docs, //! for module/crate docs
cargo doc


"Document the ./src/firestore public functions. You can use the original Javascript descriptions of the functions,
▌ found in ./packages/firestore and ./packages/firebase/firestore folders, and in the ./docs-devsite/firestore* files"


Following the instructions in ./AGENTS.md, implement the StorageReference operations for the module storage in ./
▌ packages/storage

## Modules:

- Firebase is the API

Stable/full porting:

- app
- auth
- firestore
- storage

Minimal porting:

- ai
- analytics
- app_check
- data-connect
- database
- functions
- intallations
- messaging
- performance
- remote-config


