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


 To run the new browser-based tests locally, build them with 
  - cargo test --target wasm32-unknown-unknown --features wasm-web --no-run
  
  , then execute via wasm-bindgen-test e.g.: 
   - wasm-pack test --headless --chrome -- --features wasm-web


cargo test --target wasm32-unknown-unknown --features wasm-web -- --test wasm_smoke


rustup target add wasm32-unknown-unknown        # once
  cargo test --target wasm32-unknown-unknown --features wasm-web -- --test wasm_smoke

  (or use wasm-pack test --headless --chrome --features wasm-web).


## how do I install the wasm-pack?

You can grab the wasm-pack release binaries straight from the Rust toolchain team:

  1. Install it (one-liner using the official installer script):

It's moved to another github, easier to isntall with rustup

     curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

     On Windows (PowerShell) you can run:

     iwr https://rustwasm.github.io/wasm-pack/installer/init.ps1 -useb | iex

     (Both scripts drop a prebuilt binary into ~/.cargo/bin so it’s on your PATH.)
  2. Verify it:

     wasm-pack --version

  That’s it—no extra packages required. After that you can run wasm-pack test --headless --chrome
  -- --features wasm-web to exercise the new wasm smoke tests.

cargo check
cargo check --tests
cargo check --tests --target wasm32-unknown-unknown --features wasm-web
cargo check --tests --target wasm32-unknown-unknown --features wasm-web
cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
./scripts/smoke.sh (native portion runs; wasm smoke test is skipped with a warning because wasm-bindgen-test-runner
  is not available in this environment)



### Check for dead_code

Fai un search for dead_code per capire se serve ancora, è stato messo per tenere pulito il porting

