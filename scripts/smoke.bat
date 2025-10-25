@echo off
setlocal EnableDelayedExpansion

set "REPO_ROOT=%~dp0.."
cd /d "%REPO_ROOT%"

call :run cargo fmt --all -- --check
call :run cargo test --tests -- --skip native:: --skip native_tests:: --skip messaging::api::tests::get_token_with_empty_vapid_key_returns_error

rustup target list --installed | findstr /c:"wasm32-unknown-unknown" >nul
if errorlevel 1 (
    echo error: wasm32-unknown-unknown target not installed. Run "rustup target add wasm32-unknown-unknown" first.
    exit /b 1
)

call :run cargo check --target wasm32-unknown-unknown --features wasm-web

findstr /c:"pub mod app_check {}" src\lib.rs >nul
if not errorlevel 1 (
    echo ==> skipping wasm smoke tests (app_check still stubbed on wasm; see TODO(async-wasm) note)
) else (
    call :run cargo test --target wasm32-unknown-unknown --features wasm-web wasm_smoke
)

echo Smoke tests completed
exit /b 0

:run
set "CMD=%*"
echo ==> %CMD%
%CMD%
if errorlevel 1 exit /b 1
goto :eof
