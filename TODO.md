# Miscellaneous TODO

## Create a RUSTDOC.md with extracts from README.md


Create a slimmer, less noisy RUSTDOC.md file for each module, with sections extracted from the official module's README.md (better if through a script) to be included as a DOC in the file, less crowded that the README.md file

#![doc = include_str!("RUSTDOC.md")]


## Async installations for messaging

 Added a detailed plan to src/installations/README.md outlining how to make Installations wasm-capable:

  - split the REST client into native and window.fetch implementations, keep APIs async, and document the approach
  - add IndexedDB + BroadcastChannel persistence for wasm while keeping file-backed storage for native
  - expose installation credentials so messaging can call the FCM REST endpoints once the wasm client lands
  - note follow-on parity tasks (onIdChange, retry/backoff, heartbeat, etc.)

  We're ready to start fresh with that roadmap.


 cargo check
  - cargo check --target wasm32-unknown-unknown --features wasm-web
