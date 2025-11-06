# Miscellaneous TODO

Dump file, ignore it


## Failed tests

cargo test - su run separati

---- functions::api::tests::https_callable_invokes_backend stdout ----

thread 'functions::api::tests::https_callable_invokes_backend' panicked at src\functions\api.rs:424:14:
called `Result::unwrap()` on an `Err` value: FunctionsError { code: Internal, message: "Service functions is not available", details: None }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    functions::api::tests::https_callable_invokes_backend


---- child_added_listener_reports_new_children stdout ----

thread 'child_added_listener_reports_new_children' panicked at tests\database_listeners.rs:60:49:
called `Result::unwrap()` on an `Err` value: DatabaseError { code: Internal, message: "Database component not available" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    child_added_listener_reports_new_children

## check tests


       2. Check all targets, including unit tests:

              cargo check --all-targets --profile=test

## Blocking

In README.md

check app_check::types::box_app_check_future for 2 impl - WASM and non-WASM blocking

check firestore::datastore::box_stream_future

## Public API export and maintenance

- ai
- analytics
- app
- app_check OK
  - persistence:: - used outside of module?
  - recaptcha:: - used outside of module?
  - refresher:: - used outside of module?
  - state:: - used outside of module?
  - token_provider:: - used only for firestore, gate behind firestore feature?
- auth OK
  -auth::api(core)::auth_for_app (core?) - è tipo get_auth in JS SKD

- blocking
- (component: internal)
- data_connect
- database
- firestore

database_id::DatabaseId
document_key::DocumentKey;
field_path::{FieldPath, IntoFieldPath};
geo_point::GeoPoint;
resource_path::ResourcePath;
timestamp::Timestamp;


  - query::QueryDefinition is public but its methods are pub(crate)
  - there are some pub(crate) that might be public API instead
- functions
- installations
- logger
- messaging
- performance
- platform NO
- remote_config
- storage
- test_support NO
- util NO


## doctest_support

Fare doctest_support in modo che gli esempi siano compilabili


## WASM and async

WASM parity 
Read ./AGENTS.md for context. Our focus now it to make the library and each module async and wasm-compatible,
▌ eliminating all the blocks. Breaking the API is OK. Read the WASM_PLAN.md and work on the module Functions


C'è _async in functions

Check that there is adequate testing for wasm (see for example ./src/functions/api.rs che tutti i test sono dietro gate not wasm32)

Fare anche un search per wasm32 per vedere quali parti di codice sono bloccate


## Test fallito forse race condition perché ogni tanto fallisce ogni tanto no

failures:

---- functions::api::tests::https_callable_includes_context_headers stdout ----

thread 'functions::api::tests::https_callable_includes_context_headers' panicked at src\functions\api.rs:460:14:
called `Result::unwrap()` on an `Err` value: FunctionsError { code: Internal, message: "Service functions is not available", details: None }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


## examples ignored

trasformarli in examples compiled-not run


## contare linee di codice


come posso contare le linee di codice in un progetto Rust? Ci sono tool o script (anche python)?

Ecco i metodi più usati (da super veloce a più “su misura”) per contare le linee in un progetto Rust.

Strumenti pronti (consigliati)

1) tokei (Rust, velocissimo)

# installazione
cargo install tokei
# uso nella root del repo
tokei
# per una singola cartella
tokei src


Mostra file, linguaggio, code/commenti/blank e rispetta in genere i pattern ignorati comuni.


auth/README.md what's implemented


## Blocking

Reqwest blocking:

blocking::request::ReqestBuilder

    /// Constructs the Request and sends it the target URL, returning a Response.
    pub fn send(self) -> crate::Result<super::Response> {
        self.client.execute(self.request?)
    }

blocking::client::Client

    /// Executes a `Request`.
    pub fn execute(&self, request: Request) -> crate::Result<Response> {
        self.inner.execute_request(request)
    }


pub struct Client {
    inner: ClientHandle,
}

Fa una async closure e le passa a blocking::wait::timeout(f, timeout)
blocking::client::Client::ClientHandle
    // https://docs.rs/reqwest/latest/src/reqwest/blocking/client.rs.html::1434
    fn execute_request(&self, req: Request) -> crate::Result<Response> {
        let result: Result<crate::Result<async_impl::Response>, wait::Waited<crate::Error>> =
            if let Some(body) = body {
              ...
            } else {
                let f = async move { rx.await.map_err(|_canceled| event_loop_panicked()) };
                wait::timeout(f, timeout)
            };


Da studiare blocking::wait::timeout(f, timeout)





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

#[allow(unused_imports)]

### Conditional, some of them are not ok


./src/functions/context.rs:98
#[cfg(not(all(feature = "wasm-web", target_arch = "wasm32")))]

Non so perché quel pezzo di codice non viene letto, contiene errori ma non viene segnalato da nessun cargo check

