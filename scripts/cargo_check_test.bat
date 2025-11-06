@echo off

setlocal EnableDelayedExpansion

set "REPO_ROOT=%~dp0.."
cd /d "%REPO_ROOT%"

echo running cargo check --all-targets --profile=test:
cargo check --all-targets --profile=test
if errorlevel 1 (
    echo error: cargo check --all-targets --profile=test failed
    exit /b 1
)


echo running cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web --profile=test:
cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web --profile=test
if errorlevel 1 (
    echo error: cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web --profile=test failed
    exit /b 1
)

echo running cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db --profile=test:
cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db --profile=test
if errorlevel 1 (
    echo error: cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db --profile=test failed
    exit /b 1
)
