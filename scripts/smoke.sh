#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "==> cargo test --tests -- --skip native:: --skip native_tests:: --skip messaging::api::tests::get_token_with_empty_vapid_key_returns_error --skip messaging::api::tests::on_background_message_returns_sw_error_on_non_wasm"
cargo test --tests -- --skip native:: --skip native_tests:: --skip messaging::api::tests::get_token_with_empty_vapid_key_returns_error --skip messaging::api::tests::on_background_message_returns_sw_error_on_non_wasm

if ! rustup target list --installed | grep -q '^wasm32-unknown-unknown$'; then
    echo "error: wasm32-unknown-unknown target not installed. Run 'rustup target add wasm32-unknown-unknown' first." >&2
    exit 1
fi

echo "==> cargo check --target wasm32-unknown-unknown --features wasm-web"
cargo check --target wasm32-unknown-unknown --features wasm-web

if command -v wasm-bindgen-test-runner >/dev/null 2>&1; then
    echo "==> cargo test --target wasm32-unknown-unknown --features wasm-web wasm_smoke"
    cargo test --target wasm32-unknown-unknown --features wasm-web wasm_smoke
else
    echo "warning: wasm-bindgen-test-runner not found; skipping wasm smoke tests"
fi

echo "Smoke tests completed"
