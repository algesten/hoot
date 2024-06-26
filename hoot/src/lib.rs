//! Sans-IO http 1.1 library.
//!

#![forbid(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::uninlined_format_args)]
// #![deny(missing_docs)]

#[macro_use]
extern crate log;

// Re-export the basis for this library.
pub use http;

mod error;
pub use error::Error;

mod ext;
mod chunk;
mod util;

mod body;

pub mod client;

mod parser;
