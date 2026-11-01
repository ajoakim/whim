//! Whim library crate: the app is a thin binary over these modules.

#[cfg(not(target_pointer_width = "64"))]
compile_error!("Whim requires a 64-bit target (u64 file offsets are cast to usize).");

pub mod view;
