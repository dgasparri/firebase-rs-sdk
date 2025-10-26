@echo off

setlocal EnableDelayedExpansion

set "REPO_ROOT=%~dp0.."
cd /d "%REPO_ROOT%"

echo running cargo test --lib:
cargo test --lib
if errorlevel 1 (
    echo error: cargo test --lib failed
    echo try: cargo test --lib -- --test-threads=1
    exit /b 1
)


echo running cargo test --lib --target wasm32-unknown-unknown --features wasm-web:
cargo test --lib --target wasm32-unknown-unknown --features wasm-web
if errorlevel 1 (
    echo error: cargo test --lib --target wasm32-unknown-unknown --features wasm-web failed
    exit /b 1
)

echo running cargo test --lib --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db:
cargo test --lib --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
if errorlevel 1 (
    echo error: cargo test --lib --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db failed
    exit /b 1
)

