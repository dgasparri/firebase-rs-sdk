#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

run() {
    echo "running $*"
    if ! "$@"; then
        echo "error: $* failed" >&2
        exit 1
    fi
}

run cargo check --all-targets
run cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web
run cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
