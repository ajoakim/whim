//! Whim library crate: the app is a thin binary over these modules.

#[cfg(not(target_pointer_width = "64"))]
compile_error!("Whim requires a 64-bit target (u64 file offsets are cast to usize).");

pub mod app;
pub mod args;
pub mod backend;
pub mod bench;
pub mod copy;
pub mod dialogs;
pub mod doc;
pub mod filesource;
pub mod fonts;
pub mod goto;
pub mod ime;
pub mod lineindex;
pub mod open;
pub mod scroll;
pub mod selection;
pub mod source;
pub mod status;
pub mod text;
pub mod theme;
pub mod view;

#[cfg(test)]
pub mod testutil;
