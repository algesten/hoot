//! Sans-IO http 1.1 library.
//!

#![forbid(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::uninlined_format_args)]

#[macro_use]
extern crate log;

// Re-export the basis for this library.
pub use http;

mod error;
pub use error::Error;

mod analyze;
mod chunk;
mod util;

mod body;

mod client;
pub use client::Call;
