use futures::io::AsyncRead;

#[cfg(not(target_arch = "wasm32"))]
pub trait UploadAsyncRead: AsyncRead + Unpin + Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T> UploadAsyncRead for T where T: AsyncRead + Unpin + Send {}

#[cfg(target_arch = "wasm32")]
pub trait UploadAsyncRead: AsyncRead + Unpin {}
#[cfg(target_arch = "wasm32")]
impl<T> UploadAsyncRead for T where T: AsyncRead + Unpin {}
