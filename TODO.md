# Miscellaneous TODO

Dump file, ignore it


## Public API export and maintenance

- ai
- analytics
- app OK
  - ./src/app/registry.rs is imported as pub(crate), but some methods are only pub and others are pub(crate) - is it useful as public API?
- app_check OK
  - persistence:: - used outside of module?
  - recaptcha:: - used outside of module?
  - refresher:: - used outside of module?
  - state:: - used outside of module?
  - token_provider:: - used only for firestore, gate behind firestore feature?
- auth OK
  - auth::api(core)::auth_for_app (core?) - is get_auth in JS SKD?
- blocking
- (component: internal)
- data_connect
- database
- firestore - OK
  - query::QueryDefinition is public but its methods are pub(crate)
  - there are some pub(crate) that might be public API instead
- functions
- installations - OK
- logger
- messaging - OK
- performance
- platform NO
- remote_config 
- storage - OK
- test_support NO
- util NO


## doctest_support

Fare doctest_support in modo che gli esempi siano compilabili


## Examples

Implement more examples


### unexpected feature

Check what's inside cargo.toml[features]

	"message": "unexpected `cfg` condition value: `doc-test-support`\nexpected values for `feature` are: `ai-http`, `default`, `experimental-indexed-db`, `firestore`, `js-sys`, `wasm-bindgen`, `wasm-bindgen-futures`, `wasm-web`, and `web-sys`\nconsider adding `doc-test-support` as a feature in `Cargo.toml`\nsee <https://doc.rust-lang.org/nightly/rustc/check-cfg/cargo-specifics.html> for more information about checking conditional configuration\n`#[warn(unexpected_cfgs)]` on by default",

## WASM and async

WASM parity 
Read ./AGENTS.md for context. Our focus now it to make the library and each module async and wasm-compatible,
▌ eliminating all the blocks. Breaking the API is OK. Read the WASM_PLAN.md and work on the module Functions


There is _async in some functions

Check that there is adequate testing for wasm (see for example ./src/functions/api.rs all tests are gated not wasm32)



## Failed cargo test - race condition? From time to time those fails

failures:

---- functions::api::tests::https_callable_includes_context_headers stdout ----

thread 'functions::api::tests::https_callable_includes_context_headers' panicked at src\functions\api.rs:460:14:
called `Result::unwrap()` on an `Err` value: FunctionsError { code: Internal, message: "Service functions is not available", details: None }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


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



---- value_listener_emits_initial_and_updates stdout ----

thread 'value_listener_emits_initial_and_updates' panicked at tests\database_listeners.rs:32:49:
called `Result::unwrap()` on an `Err` value: DatabaseError { code: Internal, message: "Database component not available" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

failures:
    value_listener_emits_initial_and_updates





## count lines of code


cargo install tokei

tokei

or, for a single folder:

tokei src



## Blocking

feature experimental-blocking

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


blocking::wait::timeout(f, timeout)

To put a note in README.md

check app_check::types::box_app_check_future for 2 impl - WASM and non-WASM blocking

check firestore::datastore::box_stream_future




## Clean the README.md and give them a schema 

Create a slimmer, less noisy RUSTDOC.md file for each module, with sections extracted from the official module's README.md (better if through a script) to be included as a DOC in the file, less crowded that the README.md file

#![doc = include_str!("RUSTDOC.md")]



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

Search for dead_code and unused_imports to understand if it's used. Sometimes it's dead code only in wasm32

