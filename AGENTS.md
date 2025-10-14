# Repository Guidelines

## Project Structure & Module Organization

The repo is an attempt to port to Rust the Firebase JavaScript SDK. The original SDK Javascript code lives in `./packages/{module}`, while the ported Rust code is in  `./src/{module}`.

For each submodule, ignore all the `.eslintrc.js`, `api-extractor.json`, `karma.conf.js`, `rollup.config.js` and `tsconfig.json` files. 

If common services are needed, they should NOT be in the folder of the single module but rather at the root of the Rust code ./src or in a separate directory.

Try to adhere as much as possible to the public JS APIs, so that it is easy for a programmer to look at the JS documentation and infer how the Rust APIs should work. But the code behind the API should be using Rust logic and Rust's specific way of programming. Ignore all the JS specific requirements, or if useful code them in a way that an experienced Rust programmer would code them. If popular and well known Rust library exists for some specific tasks (such as base64, reqwest, etc.) feel free to use those.

## Features of the JS APIs that are specific for the web/browser environment

If some of the features of the JS APIs are specific for the web/browser environment, port them to Rust if they could be useful in a WASM module, and port them in a way that they could be used in a WASM module. Make them available only when the library is compiled with the  'wasm-web' or equivalent feature. 

The Firebase JS SDK is a reference, but if some features are not available to the Rust language or environment, we implement them only in the part that makes it easy for the end user to implement those features outside of Rust. 

Do NOT implement the Javascript side code to use them - this is left to the user. We are building a Firebase SDK library, not a final product, we do not want to implement anything that should be implemented by the user of the library. 


For that reason, we do not write Javascript code just to make the library 100% feature equal to the Javascript library. If we can make the end user's life easier, we can start to draft an interface or something to implement that feature, but leaving everything outside Rust to the user of the library. 

## File README.md for each module

For each module there is a file `./src/{module}/README.md`. This file has 5 sections: 
 - Introduction 
 - Quick Start Example
 - Implemented
 - Still to do
 - Next steps - Detailed completion plan

The "Indroduction" section has a brief description of the module.

The Quick STart Example has a quick example on how to use the APIs of the module.

The "Implemented" section has a description of the features that are already implemented for that module.

The "Still to do" section has a list of features that are yet to be implemented to reach parity with the JS SDK library.

The "Next Steps - Detailed Completion Plan" has a detailed plan to further the porting of the module. The plan can be partial, regarding only some of the features in the "Still to do" section, but it must contain detailed, actionable steps to move the porting forward.

Refer to each module's  `./src/{module}/README.md` README.md file when working on a module. 

The README.md must be created for the modules that do not have it, and it must always be updated every time a feature is ported or a step is completed. In general, the README.md for each module must be kept up to date with all the relevan informations about that module.

Ignore the files named LOG.md in any folder they are found, they are intended only for human recall and can provide false, inaccurate, irrelevant or not updated information.

## Coding Style & Naming Conventions

When touching Rust code, format with `cargo fmt` and follow idiomatic module naming (`snake_case` files, `CamelCase` types). Try to adhere as much as possible to original Javascript public APIs and folder structure, but make the format Rust-like and adherent to Rust logic.

## Documenting the code

The public APIs must be documented in rustdoc format. 

The APIs documentation should be generated considering the `./packages/{module}` code and documentation, the `./packages/firebase/{module}` code and documentation, and the `./docs-devsite/{module}.*` files.

In the documentation of the APIs add a short usage example when it could help to clarify the usage of that function.

When available, for private functions, modules and data types that correspond to analog JS code, write in a comment a reference to where the JS corresponding code would be. If more information on a function are needed, feel free to document also that function.

## Examples

Examples that cannot be contained in the rustdoc of a function should be placed in the `./example/{module}/` folder, one folder per module. Write a simple example whenever that could help the end user. 

## Testing Guidelines

Tests should be imported in the way Rust requires them, at the end of the single file for unit tests, and in the `./tests` folder for tests that work on more than one module, and for helpers.

Author unit tests alongside the source and mirror existing fixtures and tests. 

## Commit & Pull Request Guidelines

History is minimal (`.gitignore`), so establish clarity: write imperative subjects under 72 characters and group related changes per commit. Prefer Conventional Commit prefixes (`{module}:`, `feat:`, `fix:`, `chore:`) when work spans multiple packages. Pull requests should describe the affected services, note any build/test commands executed, link tracking issues, and attach test output snippets. Flag breaking changes prominently and call out follow-up work or TODO markers.


