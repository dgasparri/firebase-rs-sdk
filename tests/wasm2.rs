#![cfg(all(target_arch = "wasm32", feature = "wasm-web"))]

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);
