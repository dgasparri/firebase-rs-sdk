# Firebase rs SDK Unofficial

This is a unofficial port of the Google's Firebase JS SDK. The goal is to mirror the features offered by the JavaScript SDK while exposing idiomatic Rust APIs.

**A note from the author:** Firebase was launched in 2011 and later acquired by Google. It's a shame that in 2025 there is still no official Rust SDK from Google to use the service. This is an attempt to fill that void, hoping that Google will soon create an official Rust SDK, either by starting with this library or from scratch.

This library is mainly **AI generated**. It is built by milking my ChatGPT subscription for all it's worth. I tried to instruct ChatGPT to adhere as much as possible to the original API structure and naming, so that the official JS SDK documentation could be a source of information useful also for the Rust SDK. 

At the time of writing (15th October 2025) some modules (Firestore database, Storage) are quite complete and already in use. Some other modules (App, App Check, Auth) are mainly developed and ready to use, but have not been tested by me. Other module (Ai, analytics...) only the basic functions are ported, and I am waiting to have ChatGPT resources to finish them. For all the modules there is an attempt to document the API and to port also the tests. All the code published passes `cargo test`. Beware that we still need to check if all the relevant tests have been ported from the JS SDK, and if the tests cover all the important aspects of the library.

If you want to contribute, your AI resources are the best thing you can donate to this project. See the [CONTRIBUTING] page on how to help.

**Why the JS SDK as a source?** Firebase has several official SDKs. From the point of view of the language architecture, the cpp version was probably a better source, but the JS was one of the few that implemented the services from scratch without relying on some Java external library. Besides, it is one of the most complete and best documented APIs. 

Resources for the Firebase JS SDK:

- Quickstart Guide: <https://firebase.google.com/docs/web/setup>
- API references: <https://firebase.google.com/docs/reference/js/>
- SDK Github repo: <https://github.com/firebase/firebase-js-sdk>

This material is from Google and the Community.

## Modules

Stable/fully ported (vast majority of features and tests ported, API calls are documented, there are working examples):

- [app](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app)
- [auth](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/auth)
- [auth_check](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/auth_check)
- [firestore](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/firestore)
- [storage](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/storage)

Minimal porting (some basic features and tests are ported, some API call documentation is missing, there is no working , API calls could evolve significantly):

- [ai](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/ai)
- [analytics](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/analytics)
- [app_check](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/app_check)
- [data-connect](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/data_connect)
- [database](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/database)
- [functions](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/functions)
- [intallations](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/installations)
- [messaging](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/messaging)
- [performance](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/performance)
- [remote-config](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/remote_config)


Modules used internally by the library but with no direct API exposure (only the relevant features are ported on a need basis):

- [component](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/component)
- [logger](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/logger)
- [util](https://github.com/dgasparri/firebase-rs-sdk-unofficial/tree/main/src/util)






## Evolution, breaking changes, breaking changes

The plan for this library is the following:

- Version 0.xx: some of the modules are partially developed
- Version 1.xx: all the modules are at a mature stage of development
- Version 2.xx: the API interface is rationalized and the tests are checked to cover the fundamental aspects of the library

For the mature modules (Auth, Firestore, Storage), we do not expect breaking changes between our current version and the 1.xx, but it is possibile that from the 1.xx to the 2.xx there will be breaking changes due to renaming and reorganizing the public API. 

# Esample



read and follow the insturctions in ./AGENTS.md . In the last session we were working on the storage module, and you
▌ suggested to "Implement Auth/App Check token retrieval and attach headers during request execution." Do it

## Example




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


## How to contribute



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
