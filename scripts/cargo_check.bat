@echo off

setlocal EnableDelayedExpansion

set "REPO_ROOT=%~dp0.."
cd /d "%REPO_ROOT%"

echo "running cargo check:"
cargo check 
pause
echo "running cargo check --tests:"
cargo check --tests
pause
echo "running cargo check --target wasm32-unknown-unknown --features wasm-web:"
cargo check --target wasm32-unknown-unknown --features wasm-web
pause
echo "running cargo check --target wasm32-unknown-unknown --features wasm-web --tests:"
cargo check --target wasm32-unknown-unknown --features wasm-web --tests
pause
echo "running cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db:"
cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db
pause
echo "running cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db --tests:"
cargo check --target wasm32-unknown-unknown --features wasm-web,experimental-indexed-db --tests
