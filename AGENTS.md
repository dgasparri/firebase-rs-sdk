# Repository Guidelines

This repository ports the Firebase JavaScript SDK to Rust. These instructions keep modules consistent, WASM-friendly, and easy to cross-reference with the original JS code.

## Structure & Organization

- JS sources live in `./packages/{module}`; Rust ports live in `./src/{module}`.
- Ignore module-local config files: `.eslintrc.js`, `api-extractor.json`, `karma.conf.js`, `rollup.config.js`, and `tsconfig.json`.
- Shared services belong at `./src` or another shared directory, not inside a specific module.
- The library must build for `wasm32-unknown-unknown` when `wasm-web` and `experimental-indexed-db` are enabled. Favor shared abstractions so native and WASM paths diverge only when necessary.
- Cross-platform helpers live in `./src/platform`. Extend existing helpers instead of duplicating WASM-specific code. Example: messaging’s IndexedDB bindings should be reused by installations, app-check, etc.
- Match JS public APIs where practical, but implement them idiomatically in Rust. Use well-known Rust crates where appropriate.
- Keep `./src/{module}/mod.rs` clean: only re-export the public API with `pub use` and inline docs. Do not expose public items directly from inner modules; consumers should import exclusively from `src/{module}/mod.rs`.

## Web-Only JS Features

- Port browser-only JS features when they are useful for WASM; gate them behind `wasm-web` (and related) features.
- If the environment cannot support a JS feature, expose only the Rust side that helps users integrate their own JS. Do not ship JS glue code. Interfaces are acceptable; implementation outside Rust is the user’s job.

## Module README requirements

Each module has `./src/{module}/README.md` with these sections:

- Firebase {module} (brief description + porting percentage)
- Features (optional)
- Quick Start Example
- Quick Start Example Using … (one or more subsections, optional)
- References to the Firebase JS SDK
- Intentional deviations from the JS SDK (optional)
- WASM Notes (optional)

The "Intentional deviations from the JS SDK" section reports the cases where it was deemed useful to deviate from the behavior of the JS SDK public API.

The "WASM Notes" section contains any information that could be useful when compiling with target wasm32.

The README is updated manually, except that “Intentional deviations…” and “WASM Notes” may be updated by AI when needed.

## Module PORTING_STATUS requirements

Each module has `./src/{module}/PORTING_STATUS.md` with:

- Porting status (percentage; update only after substantial progress)
- Implemented (features already ported)
- Still to do (features needed to match the JS SDK)
- Next Steps – Detailed Completion Plan (actionable steps to advance the port)

Keep this file current whenever features are added or steps are completed.

## Coding Style & Documentation

- Run `cargo fmt`; use idiomatic naming (`snake_case` files, `CamelCase` types).
- Public APIs must have rustdoc comments. Include short usage examples when helpful.
- Derive docs from `./docs-devsite/{module}*`, `./packages/{module}`, and `./packages/firebase/{module}`. For private items mirroring JS, add a brief comment pointing to the analogous JS code when useful.

## Testing Guidelines

- Place unit tests in the same file (at the end); cross-module or helper tests go under `./tests`.
- Port relevant JS SDK tests. Use async-aware harnesses (e.g., `#[tokio::test]`) and `await` futures; always avoid `block_on` to prevent runtime conflicts.

## Commit & PR Guidelines

- Use imperative subjects under 72 characters; prefer Conventional Commit prefixes (`{module}:`, `feat:`, `fix:`, `chore:`) when changes span modules.
- PRs should describe affected services, commands/tests run, linked issues, and include test output snippets. Flag breaking changes and note follow-ups/TODOs.

## Job-Specific Instructions

### Porting TS code to Rust

- Study the relevant TS in `./packages/{module}` and `./packages/firebase/{module}`. Mirror API names and shapes where it helps JS parity, but keep Rust idioms.
- Use each module’s `./src/{module}/PORTING_STATUS.md` to understand gaps; update it as work progresses.

### Writing documentation

- Sources: `./docs-devsite/{module}*`, TS code/comments, and your Rust implementation.
- Use rustdoc; include concise examples when they clarify usage.
- When applicable, mention the TS function you ported.

### Writing examples

- Save examples to `./examples` named `{module}_{function}.rs`.
- If mocks or local services are used, note in comments how to switch to real Firebase services.
- Tiny, function-specific examples can live in rustdoc. Keep them minimal and omit boilerplate. Shared helpers can go in `./src/dotest_support.rs` (reference-only).

### Testing

- Use `cargo test`. Port relevant TS tests for each module and ensure they pass.

### Updating PORTING_STATUS.md

- Follow the required sections above. Compare TS (`./packages/{module}`) and Rust (`./src/{module}`) to reflect implemented and missing features, and refresh the completion plan.

### PR messaging

- PR titles start with the affected module name and summarize changes. The description should detail code changes, benefits, and any API-breaking changes.

### Async traits on WASM

- If `async_trait` adds `Send` bounds that break `wasm32`, use the pattern from `src/app_check/types.rs`:
  - Define a target-aware alias (e.g., `StreamingFuture`) switching between `LocalBoxFuture` and `BoxFuture`.
  - Provide helpers (e.g., `box_stream_future`) to box futures without `async_trait`.
  - Have traits return the alias instead of `async fn` to stay WASM-compatible.

## Miscellaneous

- Ignore `AI_LOG.md` and `LOG.md`; they are informal and may be outdated.
