# Repository Guidelines

## Project Structure & Module Organization

The repo is an attempt to port to Rust the Firebase JavaScript SDK. The original SDK Javascript code lives in `./packages/{module}`, while the ported Rust code is in  `./src/{module}`.

For each submodule, ignore all the `.eslintrc.js`, `api-extractor.json`, `karma.conf.js`, `rollup.config.js` and `tsconfig.json` files. 

If common services are needed, they should NOT be in the folder of the single module but rather at the root of the Rust code ./src or in a separate directory.

The library must build for the `wasm32-unknown-unknown` (aka `wasm-unknown-unknown`) target when compiled with the `wasm-web` and `experimental-indexed-db` features enabled. Prefer shared abstractions over copy-pasting logic so the native and wasm code paths only diverge where platform differences truly require it.

### Shared platform utilities

Cross-module helpers that abstract browser/native differences are collected under `./src/platform`. For example, the IndexedDB bindings used by messaging live there so other modules (installations, app-check, etc.) can reuse them instead of re-implementing storage glue. Before building a new WASM-specific helper, scan `./src/platform` to avoid duplicating existing work and extend it in place when the functionality is broadly useful.

Try to adhere as much as possible to the public JS APIs, so that it is easy for a programmer to look at the JS documentation and infer how the Rust APIs should work. But the code behind the API should be using Rust logic and Rust's specific way of programming. Ignore all the JS specific requirements, or if useful code them in a way that an experienced Rust programmer would code them. If popular and well known Rust library exists for some specific tasks (such as base64, reqwest, etc.) feel free to use those.

Each module's root file (`./src/{module}/mod.rs`) must remain tidy and only re-export the public API surface using `pub use` items accompanied by inline documentation. All types and functions that form the public API should be referenced through this file rather than being defined there directly. All types and functions that form the public API should be made public only through this file. **Do not expose public items from inner module files; structure every module so consumers only need to import from `src/{module}/mod.rs`.**

## Features of the JS APIs that are specific for the web/browser environment

If some of the features of the JS APIs are specific for the web/browser environment, port them to Rust if they could be useful in a WASM module, and port them in a way that they could be used in a WASM module. Make them available only when the library is compiled with the  'wasm-web' or equivalent feature. 

The Firebase JS SDK is a reference, but if some features are not available to the Rust language or environment, we implement them only in the part that makes it easy for the end user to implement those features outside of Rust. 

Do NOT implement the Javascript side code to use them - this is left to the user. We are building a Firebase SDK library, not a final product, we do not want to implement anything that should be implemented by the user of the library. 

For that reason, we do not write Javascript code just to make the library 100% feature equal to the Javascript library. If we can make the end user's life easier, we can start to draft an interface or something to implement that feature, but leaving everything outside Rust to the user of the library. 

## File README.md for each module

For each module there is a file `./src/{module}/README.md`. This file has 5 sections: 

 - Firebase {module}
   - Features
   - Quick Start Example
    - Quick Start Example Using ... (multiple, optional)
   - References to the Firebase JS SDK
   - Intentional deviations from the JS SDK (optional)
   - WASM Notes (optional)

The "Firebase {module}" section has a brief description of the module, and a line with the percentage of the porting status.

The "Quick Start Example" has a quick example on how to use the APIs of the module.

Further quick start examples highliting certain features must go in separate subchapters named "Quick Start Example Using..." 

The "References to the Firebase JS SDK" section has a list of links to documentation of other Firebase SDKs. 

The "Intentional deviations from the JS SDK" section reports the cases where it was deemed useful to deviate from the behavior of the JS SDK public API. 

The "WASM Notes" section contains any information that could be useful when compiling with target wasm32.

The README.md file is updated manually, with the exceptions of the sections "Intentional deviations from the JS SDK" and "WASM Notes" that could be updated by AI when needed.

## File PORTING_STATUS.md for each module

For each module there is a file `./src/{module}/PORTING_STATUS.md`. This file has 4 sections: 

- Porting status
- Implemented
- Still to do
 - Next steps - detailed completion plan

The "Porting status" section has a reference to the advancement status (in percentage) of the porting of functions/code from the Firebase JS SDK. This section is updated only when a cospicuous work has been done on the module. 

The "Implemented" section has a description of the features that are already implemented for that module.

The "Still to do" section has a list of features that are yet to be implemented to reach parity with the JS SDK library.

The "Next Steps - Detailed Completion Plan" has a detailed plan to further the porting of the module. The plan can be partial, regarding only some of the features in the "Still to do" section, but it must contain detailed, actionable steps to move the porting forward.

Refer to each module's  `./src/{module}/PORTING_STATUS.md` README.md file when working on a module. 

The PORTING_STATUS.md must must always be updated every time a feature is ported or a step is completed. In general, the PORTING_STATUS.md for each module must be kept up to date with all the relevan informations about that module.

Ignore the files named AI_LOG.md and LOG.md in any folder they are found, they are intended only for human recall and can provide false, inaccurate, irrelevant or not updated information.

## Coding Style & Naming Conventions

When touching Rust code, format with `cargo fmt` and follow idiomatic module naming (`snake_case` files, `CamelCase` types). Try to adhere as much as possible to original Javascript public APIs and folder structure, but make the format Rust-like and adherent to Rust logic.

## Documenting the code

The public APIs must be documented in rustdoc format. 

The APIs documentation should be generated considering the `./packages/{module}` code and documentation, the `./packages/firebase/{module}` code and documentation, and the `./docs-devsite/{module}.*` files.

In the documentation of the APIs add a short usage example when it could help to clarify the usage of that function.

When available, for private functions, modules and data types that correspond to analog JS code, write in a comment a reference to where the JS corresponding code would be. If more information on a function are needed, feel free to document also that function.

## Testing Guidelines

Tests should be imported in the way Rust requires them, at the end of the single file for unit tests, and in the `./tests` folder for tests that work on more than one module, and for helpers.

Author unit tests alongside the source and mirror existing fixtures and tests. 

When testing asynchronous APIs, use async-aware harnesses (e.g. `#[tokio::test]` or an equivalent executor) and `await` the futures directly. Avoid spinning up ad-hoc runtimes or calling `block_on` in tests, as that can cause runtime conflicts or deadlocks when suites run concurrently.

## Commit & Pull Request Guidelines

History is minimal (`.gitignore`), so establish clarity: write imperative subjects under 72 characters and group related changes per commit. Prefer Conventional Commit prefixes (`{module}:`, `feat:`, `fix:`, `chore:`) when work spans multiple packages. Pull requests should describe the affected services, note any build/test commands executed, link tracking issues, and attach test output snippets. Flag breaking changes prominently and call out follow-up work or TODO markers.

## Instructions specific to particular jobs

### Porting a function or a code from Typescript and writing Rust code 

You must analyse the typescript code of the relative module in `./packages/{module}` and `./packages/firebase/{module}`. When creating Rust code, try to adhere as much as possible to the names and methods used in the Typescript API so that it feels as natural as possible for the developer to switch from the JS SDK to the Rust SDK.

A list of open features still to be ported from the Firebase JS SDK is in the README.md file inside of each module `./src/{module}/PORTING_STATUS.md`. Refer to that file, and keep it updated with the steps that have been taken, the features that have been implemented and the features that are still to be ported.

### Documenting the code

When documenting the code, use as source:

1. the files in `./docs-devsite/{module}*`
2. the Typescript source code and comments of functions and methods and data types under `./packages/{module}` and `./packages/firebase/{module}`.
3. the code you wrote

The public API code must be documented using the rustdoc convention. When possible, provide also a minimal usage example of a few lines (does not need to be run or compiled).

When possible, also write the reference to the original Typescript function you ported.

### Writing examples

Examples should be saved in the folder `./examples` and named as `{module}_{function implemented}.rs`. If a mock or a local copy of a service is used, write in the comments how the code should change if the actual Firebase service is used.

Small examples relevant to only a function can also be placed in the rustdoc documentation. Those examples must be minimal, leaving out all the non-relevant code such as boilerplate, module initialization, display of results, etc. Helper functions can be stored in the `./src/dotest_support.rs` file. They are NOT expected to run, but only as a reference for the programmer that wants to use that function 

### Testing and writing tests

The code is tested using the standard rust testing engine and the `cargo test` command. For each module, review the tests of the original Firebase JS SDK in the Typescript code in ./packages/{module} and port the relevant tests to Rust. Check that the test does not fail.

### Updating the module's PORTING_STATUS.md

The PORTING_STATUS.md file for each module must follow the rules and layout set in the "File PORTING_STATUS.md for each module" section in this document. To update the PORTING_STATUS.md, review the Typescript code in ./packages/{module} and the Rust code in ./scr/{module}, and check if ./src/{module}/PORTING_STATUS.md is reporting correct and updated information. Check specifically for the features implemented and the feature still to be implemented. Make the necessary correction to bring the file up to date.

### Messages for a PULL REQUEST

Pull requests should have a title that starts with the name of the {module} affected, and a message that explains in detail what are the changes in the code and the benefits of those changes. In particular, it should highlight if the code creates breaking changes to the APIs.

### Async traits on wasm

If `async_trait` introduces `Send` bounds that break `wasm32` builds, mirror the approach in `src/app_check/types.rs`:

- Define a target-aware alias (e.g., `StreamingFuture`) that flips between `LocalBoxFuture` and `BoxFuture`.
- Expose helpers (like `box_stream_future`) to box futures without relying on `async_trait`.
- Rewrite the trait to return the alias instead of using `async fn`.

This keeps wasm-compatible async code without duplicating implementations.
