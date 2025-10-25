#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "==> cargo test --tests -- --skip native:: --skip native_tests:: --skip messaging::api::tests::get_token_with_empty_vapid_key_returns_error"
cargo test --tests -- --skip native:: --skip native_tests:: --skip messaging::api::tests::get_token_with_empty_vapid_key_returns_error

if ! rustup target list --installed | grep -q '^wasm32-unknown-unknown$'; then
    echo "error: wasm32-unknown-unknown target not installed. Run 'rustup target add wasm32-unknown-unknown' first." >&2
    exit 1
fi

echo "==> cargo check --target wasm32-unknown-unknown --features wasm-web"
cargo check --target wasm32-unknown-unknown --features wasm-web

if grep -Fq 'pub mod app_check {}' src/lib.rs; then
    echo "==> skipping wasm smoke tests (app_check still stubbed on wasm; see TODO(async-wasm) note)"
else
    echo "==> cargo test --target wasm32-unknown-unknown --features wasm-web wasm_smoke"
    cargo test --target wasm32-unknown-unknown --features wasm-web wasm_smoke
fi

echo "Smoke tests completed"
