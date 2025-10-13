# Repository Guidelines

## Project Structure & Module Organization
The repo is an attempt to port to Rust the Firebase JavaScript SDK. The original SDK Javascript code lives in `./packages/{module}`, while the ported Rust code is in  `./src/{module}`.

For each submodule, ignore all the `.eslintrc.js`, `api-extractor.json`, `karma.conf.js`, `rollup.config.js` and `tsconfig.json` files. Try to migrate the relevant `package.json` info to `Cargo.toml`.

If common services are needed, they should NOT be in the folder of the single module but rather at the root of the Rust code ./src or in a separate directory.

Try to adhere as much as possible to the public JS APIs, so that it is easy for a programmer to look at the JS documentation and infer how the Rust APIs should work. But the code behind the API should be using Rust logic and Rust's specific way of programming. Ignore all the JS specific requirements, or if useful code them in a way that an experienced Rust programmer would code them. If popular and well known Rust library exists for some specific tasks (such as base64, reqwest, etc.) feel free to use those.

IMPORTANT: if some of the features of the JS APIs are specific for the web/browser environment, port them to Rust if they could be useful in a WASM module, and port them in a way that they could be used in a WASM module. Make them available only when the library is compiled with the  'wasm-web' feature. Do not implement the Javascript side code to use them - this is left to the user. We are building a Firebase SDK library, not a final product, we do not want to implement anything that should be implemented by the user of the library. The Firebase JS SDK is a reference, but if some features are not available to the Rust language or environment, we implement them only in the part that makes it easy for the end user to implement those features outside of Rust. For example, we do not want to write Javascript code just to make the library 100% feature equal to the Javascript library. If we can make the end user's life easier, we can start to draft an interface or something to implement that feature, but leaving everything outside Rust to the user of the library. 


For each module there is a file `./src/{module}/README.md` that has a "Next steps" section highlighting some of the required next steps to complete the porting and implement all the relevant features. Sometimes there is also a "Immediate Porting Focus" section for a more detailed list of things to do. Refer to that file when working on a module. When you are finished porting a feature, always update the README.md file with the new informations.

Tests should be imported in the way Rust requires them, at the end of the single file for unit tests, and in the tests folder.

## Coding Style & Naming Conventions
When touching Rust code, format with `cargo fmt` and follow idiomatic module naming (`snake_case` files, `CamelCase` types). Try to adhere as much as possible to original Javascript APIs and folder structure, but make the format Rust-like and adherent to Rust logic.

## Testing Guidelines
Author unit tests alongside the source and mirror existing fixtures. 

## Commit & Pull Request Guidelines
History is minimal (`.gitignore`), so establish clarity: write imperative subjects under 72 characters and group related changes per commit. Prefer Conventional Commit prefixes (`feat:`, `fix:`, `chore:`) when work spans multiple packages. Pull requests should describe the affected services, note any build/test commands executed, link tracking issues, and attach test output snippets. Flag breaking changes prominently and call out follow-up work or TODO markers.
