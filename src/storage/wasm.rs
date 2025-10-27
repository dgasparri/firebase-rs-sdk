#![cfg(all(feature = "wasm-web", target_arch = "wasm32"))]

use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use crate::storage::error::{internal_error, StorageResult};

pub async fn blob_to_vec(blob: &web_sys::Blob) -> StorageResult<Vec<u8>> {
    let promise = blob.array_buffer();
    let buffer = JsFuture::from(promise)
        .await
        .map_err(|err| internal_error(format_js_error("Blob.arrayBuffer", err)))?;
    let array = js_sys::Uint8Array::new(&buffer);
    let mut bytes = vec![0u8; array.length() as usize];
    array.copy_to(&mut bytes);
    Ok(bytes)
}

pub fn uint8_array_to_vec(array: &js_sys::Uint8Array) -> Vec<u8> {
    array.to_vec()
}

pub fn bytes_to_blob(bytes: &[u8]) -> StorageResult<web_sys::Blob> {
    let chunk = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from(chunk));
    web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|err| internal_error(format_js_error("Blob constructor", err)))
}

fn format_js_error(context: &str, err: JsValue) -> String {
    let detail = err.as_string().unwrap_or_else(|| format!("{:?}", err));
    format!("{context} failed: {detail}")
}
