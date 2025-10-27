@echo off

setlocal EnableDelayedExpansion

set "REPO_ROOT=%~dp0.."
cd /d "%REPO_ROOT%"

echo running cargo test:
cargo test
if errorlevel 1 (
    echo error: cargo test failed
    echo try: cargo test -- --test-threads=1
    exit /b 1
)


echo running cargo test --target wasm32-unknown-unknown --features wasm-web:
cargo test --target wasm32-unknown-unknown --features wasm-web
if errorlevel 1 (
    echo error: cargo test --target wasm32-unknown-unknown --features wasm-web failed
    exit /b 1
)

echo running cargo test --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db:
cargo test --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
if errorlevel 1 (
    echo error: cargo test --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db failed
    exit /b 1
)


echo running cargo test --test database_listeners:
cargo test --test database_listeners
if errorlevel 1 (
    echo error: cargo test --test database_listeners failed
    exit /b 1
)


echo running cargo test --test wasm_database_listeners --target wasm32-unknown-unknown --features wasm-web:
cargo test --test wasm_database_listeners --target wasm32-unknown-unknown --features wasm-web
if errorlevel 1 (
    echo error: cargo test --test wasm_database_listeners --target wasm32-unknown-unknown --features wasm-web failed
    exit /b 1
)


echo running cargo test --test wasm_smoke --target wasm32-unknown-unknown --features wasm-web:
cargo test --test wasm_smoke --target wasm32-unknown-unknown --features wasm-web
if errorlevel 1 (
    echo error: cargo test --test wasm_smoke --target wasm32-unknown-unknown --features wasm-web failed
    exit /b 1
)
