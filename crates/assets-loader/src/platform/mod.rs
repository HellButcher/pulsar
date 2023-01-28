use std::{future::Future, pin::Pin};

use crate::path::AssetPath;

#[cfg(not(target_arch = "wasm32"))]
pub mod fs;

#[cfg(target_os = "android")]
pub mod android;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

type BoxedFuture<'l, O> = Pin<Box<dyn Future<Output = O> + Send + Sync + 'l>>;

pub trait AssetOpen {
    fn load(&self, asset_path: &AssetPath) -> BoxedFuture<'_, std::io::Result<Vec<u8>>>;
}