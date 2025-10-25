# Async/WASM Migration Checklist

This checklist summarises the conventions to follow when porting modules to the
async runtime and enabling `wasm32-unknown-unknown` support. Use it as a quick
reference before sending a pull request.

## Toolchain & Features
- `rustup target add wasm32-unknown-unknown` before running wasm checks.
- Enable `wasm-web` when building or testing for the browser target:
  `cargo check --target wasm32-unknown-unknown --features wasm-web`.
- Opt into `experimental-indexed-db` only when a module actually needs IndexedDB
  persistence; wasm builds without it must still compile using the in-memory
  fallbacks.

## API & Naming
- Prefer the same function names exposed by the Firebase JS SDK; avoid
  introducing `_async` suffixes for futures.
- Gate browser-only symbols with `#[cfg(all(feature = "wasm-web", target_arch = "wasm32"))]`
  or helper modules inside `src/platform`.
- Re-export public APIs from the module `mod.rs` so users import from
  `firebase_rs_sdk::module::*` regardless of target.

## Concurrency & Runtime
- Use async-aware primitives (`async-lock`, `futures`, `tokio` on native,
  `wasm-bindgen-futures` on wasm) rather than blocking mutexes or threads.
- Schedule background work via `platform::runtime::spawn_detached` and sleep
  via `platform::runtime::sleep` to keep implementations portable.
- When wasm code needs timer or storage helpers, place reusable glue in
  `src/platform` so other modules can depend on it.

## Feature Gating & TODO Pattern
- When a module does not yet compile on wasm, wrap its public exposure with a
  `// TODO(async-wasm): ...` comment so the workspace continues to build.
- Prefer minimal duplication: keep shared logic in common modules and only gate
  small platform-specific helpers.
- Stub browser integrations (service workers, IndexedDB, etc.) with meaningful
  errors on native targets to maintain API parity.

## Testing
- Run `./scripts/smoke.sh` (or `scripts\smoke.bat`) before opening a PR. These
  scripts chain `cargo fmt`, a trimmed native test run, `cargo check` for wasm,
  and the wasm smoke tests (skipped automatically if `wasm-bindgen-test-runner`
  is not available locally).
- Add wasm-aware unit tests using `wasm-bindgen-test` where practical. Guard
  them with `#[cfg(all(test, feature = "wasm-web", target_arch = "wasm32"))]`.

## Documentation
- Update the relevant module `README.md` whenever a wasm-specific feature is
  added or a step in `WASM_PLAN.md` is completed.
- Mention the required features (`wasm-web`, `experimental-indexed-db`, etc.) in
  user-facing docs and examples so consumers know how to enable them.

Keeping these points in mind helps ensure each module stays portable and we can
remove temporary `TODO(async-wasm)` guards as the port progresses.
