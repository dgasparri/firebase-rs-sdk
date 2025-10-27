@echo off

setlocal EnableDelayedExpansion

set "REPO_ROOT=%~dp0.."
cd /d "%REPO_ROOT%"

echo running cargo check --all-targets:
cargo check --all-targets
if errorlevel 1 (
    echo error: cargo check --all-targets failed
    exit /b 1
)



echo running cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web:
cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web
if errorlevel 1 (
    echo error: cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web failed
    exit /b 1
)


echo running cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db:
cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
if errorlevel 1 (
    echo error: cargo check --all-targets --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db failed
    exit /b 1
)
