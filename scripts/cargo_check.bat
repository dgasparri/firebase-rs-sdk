@echo off

setlocal EnableDelayedExpansion

set "REPO_ROOT=%~dp0.."
cd /d "%REPO_ROOT%"

echo running cargo check:
cargo check
if errorlevel 1 (
    echo error: cargo check failed
    exit /b 1
)

echo running cargo check --tests:
cargo check --tests
if errorlevel 1 (
    echo error: cargo check --tests failed
    exit /b 1
)


echo running cargo check --target wasm32-unknown-unknown --features wasm-web:
cargo check --target wasm32-unknown-unknown --features wasm-web
if errorlevel 1 (
    echo error: cargo check --target wasm32-unknown-unknown --features wasm-web failed
    exit /b 1
)

echo running cargo check --target wasm32-unknown-unknown --features wasm-web --tests:
cargo check --target wasm32-unknown-unknown --features wasm-web --tests
if errorlevel 1 (
    echo error: cargo check --target wasm32-unknown-unknown --features wasm-web --tests failed
    exit /b 1
)

echo running cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db:
cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
if errorlevel 1 (
    echo error: cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db failed
    exit /b 1
)

echo running cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db --tests:
cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db --tests
if errorlevel 1 (
    echo error: cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db --tests failed
    exit /b 1
)
