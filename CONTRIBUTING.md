# Contributing to the unofficial Firebase RS SDK

Any contribution to port additional features and create new tests is greatly appreciated. Because a significant portion of the code is AI generated, extra eyes on testing and correctness are especially valuable.

## Setting up the environment

To set up the development environment, first clone the GitHub repository:

> git clone <https://github.com/dgasparri/firebase-rs-sdk.git>

Cloning the Firebase JavaScript SDK repository is optional but helpful for reference:

> <https://github.com/firebase/firebase-js-sdk.git>

You may also want to copy all the files and subfolders from the JS SDK `./packages` folder into this repo’s `./packages` folder for easier reference. These files also help the AI analyze JS SDK features. Under Windows Command Line:

> XCOPY /E firebase-js-sdk\packages\* firebase-rs-sdk\packages\.

Copy the doc files in the `docs-devsite` folder as well. Those files contain documentation for the API calls. Under Windows Command Line:

> COPY firebase-js-sdk\docs-devsite\* firebase-rs-sdk\docs-devsite\.

### Setting up the testing environment

Testing for the `wasm-unknown-unknown` target and `wasm-web` feature is done with the [wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen) library.

To install the library:

> cargo install wasm-bindgen-cli

To install the WebDrivers:

- Mozilla Geckodriver (might need to install Firefox for it to work properly):
  - <https://github.com/mozilla/geckodriver/releases> or
  - `cargo install geckodriver` or
  - `sudo apt update` and `sudo apt install firefox-esr geckodriver`
- Chromedriver: <https://chromedriver.chromium.org/downloads> or `sudo apt install chromium-browser chromium-chromedriver`
- Msedgedriver: <https://developer.microsoft.com/en-us/microsoft-edge/tools/webdriver/>

### WASM build and test quickstart

The Rust crate exposes browser-specific functionality behind the `wasm-web` feature flag. Contributors should validate changes against the `wasm32-unknown-unknown` target with the following commands:

1. Ensure the toolchain wasm target is available:

   > rustup target add wasm32-unknown-unknown

2. Check that the workspace compiles for wasm with the web feature enabled:

   > cargo check --target wasm32-unknown-unknown --features wasm-web

3. Run the wasm smoke tests (powered by `wasm-bindgen-test`) in headless mode:

   > cargo test --target wasm32-unknown-unknown --features wasm-web wasm_smoke

The suite in `tests/wasm_smoke.rs` provides a minimal browser-oriented sanity check and should pass before opening a pull request.

For convenience, the repository also ships `./scripts/smoke.sh` (or `scripts\smoke.bat` on Windows) which chains the formatting check, a trimmed native test run (skipping network-bound cases), the wasm `cargo check`, and the wasm smoke test when the `app_check` module is enabled for wasm. If `wasm-bindgen-test-runner` is not installed locally, the script will emit a warning and skip the wasm test step.

## Common AI prompts to develop code/documentation for this library

Detailed instructions for the AI are given in the ./AGENTS.md file. Here are some handy prompts we commonly used to work on the library. It is not an extensive list of the prompts, but so far they have worked fine for us.

For implementing a specific feature you are interested in:

> Following the instructions in ./AGENTS.md, implement the feature {XXX} for the module {module}.

Example: Following the instructions in ./AGENTS.md, implement the StorageReference operations for the module storage.

For moving forward in the porting of a module you are interested in, leaving to the AI to decide what it should work on:

> Following the instructions in ./AGENTS.md, read in the file ./src/{module}/PORTING_STATUS.md what are the next steps and the missing features in the module {module} and work on the first step

For documenting some of the API:

> Following the instructions in ./AGENTS.md, review the Rust code for the module {module} and write or improve the appropriate documentation

For creating an example of some feature you might be interested in:

> Following the instructions in ./AGENTS.md, write an example for the module {module} demonstrating how to use the feature {feature}. Save the example in the folder ./examples with a filename starting with the {module}_ name

For porting some of the tests from the JS SDK library:

> Following the instructions in ./AGENTS.md, review the tests in the Typescript code in ./packages/{module} and port some of the relevant tests to Rust

For a failed test:

> `cargo test [--target wasm-unknown-unknown --features wasm-web]` failed at the test {name_of_test}. Here is the output of the test with the failure message: \[Content of the cargo test output\]

For a bug:

> The module {module} did not work as expected, I suspect a bug. The expected behavior of the following code is \[expected behavior\], but I obtained \[actual behavior\]

For updating the PORTING_STATUS.md of any module:

> Review the Typescript code in ./packages/{module} and the Rust code in ./scr/{module}, and check if ./src/{module}/PORTING_STATUS.md is up to date. Check specifically for the features implemented and the feature still to be implemented. Make the necessary correction to bring the file up to date.

For preparing for a PULL REQUEST:

> Write a title and a message for a pull request explaining in detail what are the changes in the code and the benefits of those changes

For having an estimate of the porting advancement

> Compare the original JS/Typescript files in ./packages/{module} and the ported files in Rust in ./src/{module}, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module. Update the README.md file accordingly.

## Before any pull request

Before any pull request, the following steps must be taken:

1. Format your code with `cargo fmt`.
2. Ask the AI to update `./src/{module}/README.md` and `./src/{module}/PORTING_STATUS.md` (as mandated in `./AGENTS.md`).
3. Run `./scripts/cargo_check[.bat|.sh]` and `./scripts/cargo_test[.bat|.sh]` and ensure all tests pass.
4. Compile the docs with `cargo doc` and verify there are no errors.
5. Ask the AI to write a pull request title and message, or write them yourself—be specific and precise.

## Bugs and erroneous documentation

Chances are, there are bugs in the code. If you find one, or if you notice that something is not documented correctly, you can open an issue on Github or submit a Pull request.

## Before you Contribute

The code you contribute MUST be licensed under Apache 2.0.

## Testing

In the analytics module a unit test that exercises the dispatcher is skipped by default unless `FIREBASE_NETWORK_TESTS=1` is set.
