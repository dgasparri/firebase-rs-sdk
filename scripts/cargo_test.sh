#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

run() {
    local hint="$1"
    shift
    echo "running $*"
    if ! "$@"; then
        echo "error: $* failed" >&2
        if [[ -n "$hint" ]]; then
            echo "$hint" >&2
        fi
        exit 1
    fi
}

run "try: cargo test -- --test-threads=1" cargo test
run "" cargo test --target wasm32-unknown-unknown --features wasm-web
run "" cargo test --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
run "" cargo test --test database_listeners
run "" cargo test --test wasm_database_listeners --target wasm32-unknown-unknown --features wasm-web
run "" cargo test --test wasm_smoke --target wasm32-unknown-unknown --features wasm-web
