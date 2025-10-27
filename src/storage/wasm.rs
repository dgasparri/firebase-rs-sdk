#![cfg(all(feature = "wasm-web", target_arch = "wasm32"))]

use futures::io::AsyncRead;
use futures::FutureExt;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use crate::storage::error::{internal_error, StorageResult};
use std::io::{Error as IoError, ErrorKind};
use std::pin::Pin;
use std::task::{Context, Poll};

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

pub(crate) fn readable_stream_async_reader(
    stream: &web_sys::ReadableStream,
) -> StorageResult<ReadableStreamAsyncReader> {
    let reader_value = stream.get_reader();
    let reader = reader_value
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|err| {
            internal_error(format_js_error("ReadableStreamDefaultReader", err.into()))
        })?;
    Ok(ReadableStreamAsyncReader::new(reader))
}

pub struct ReadableStreamAsyncReader {
    reader: web_sys::ReadableStreamDefaultReader,
    pending_future: Option<JsFuture>,
    buffer: Option<Vec<u8>>,
    buffer_offset: usize,
    done: bool,
}

impl ReadableStreamAsyncReader {
    fn new(reader: web_sys::ReadableStreamDefaultReader) -> Self {
        Self {
            reader,
            pending_future: None,
            buffer: None,
            buffer_offset: 0,
            done: false,
        }
    }
}

impl Unpin for ReadableStreamAsyncReader {}

impl AsyncRead for ReadableStreamAsyncReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, IoError>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        loop {
            if let Some(chunk) = self.buffer.as_ref() {
                let offset = self.buffer_offset;
                let len = chunk.len();
                let remaining = &chunk[offset..];
                let to_copy = remaining.len().min(buf.len());
                buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
                self.buffer_offset += to_copy;
                if self.buffer_offset >= len {
                    self.buffer = None;
                    self.buffer_offset = 0;
                }
                return Poll::Ready(Ok(to_copy));
            }

            if self.done {
                return Poll::Ready(Ok(0));
            }

            if self.pending_future.is_none() {
                let promise = self.reader.read();
                self.pending_future = Some(JsFuture::from(promise));
            }

            if let Some(future) = &mut self.pending_future {
                match future.poll_unpin(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(err)) => {
                        self.pending_future = None;
                        return Poll::Ready(Err(js_to_io_error(err)));
                    }
                    Poll::Ready(Ok(value)) => {
                        self.pending_future = None;
                        match parse_read_result(value) {
                            Ok(ReadChunk::Done) => {
                                self.done = true;
                                continue;
                            }
                            Ok(ReadChunk::Data(data)) => {
                                self.buffer = Some(data);
                                self.buffer_offset = 0;
                                continue;
                            }
                            Err(err) => return Poll::Ready(Err(err)),
                        }
                    }
                }
            }
        }
    }
}

enum ReadChunk {
    Data(Vec<u8>),
    Done,
}

fn parse_read_result(value: JsValue) -> Result<ReadChunk, IoError> {
    use js_sys::Reflect;

    let done = Reflect::get(&value, &JsValue::from_str("done"))
        .map_err(js_to_io_error)?
        .as_bool()
        .unwrap_or(false);
    if done {
        return Ok(ReadChunk::Done);
    }

    let chunk_value = Reflect::get(&value, &JsValue::from_str("value")).map_err(js_to_io_error)?;
    if chunk_value.is_undefined() || chunk_value.is_null() {
        return Ok(ReadChunk::Done);
    }

    let array = js_sys::Uint8Array::new(&chunk_value);
    let mut bytes = vec![0u8; array.length() as usize];
    array.copy_to(&mut bytes);
    Ok(ReadChunk::Data(bytes))
}

fn js_to_io_error(err: JsValue) -> IoError {
    IoError::new(ErrorKind::Other, format_js_error("ReadableStream", err))
}

fn format_js_error(context: &str, err: JsValue) -> String {
    let detail = err.as_string().unwrap_or_else(|| format!("{:?}", err));
    format!("{context} failed: {detail}")
}
