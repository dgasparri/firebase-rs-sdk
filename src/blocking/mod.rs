// #[cfg(not(target_arch = "wasm32"))]

pub mod database;

use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

macro_rules! block_on_methods {
    ($(fn $name:ident($($arg:ident : $ty:ty),*) -> $ret:ty);* $(;)?) => {
        $(pub fn $name(&self, $($arg:$ty),*) -> $ret {
            RT.block_on(self.inner.$name($($arg),*))
        })*
    };
}

static RT: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all() // timer, I/O, ecc.
        .build()
        .expect("Tokio runtime")
});

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    RT.block_on(fut)
}