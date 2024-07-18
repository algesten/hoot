//! Sans-IO HTTP/1.1 library.
//!
//! hoot is a library that implements the HTTP/1.1 protocol without considering transport.
//! The goal is to make a first class HTTP/1.1 implementation that can be used in other projects
//! that add socket handling, cookies, body compression, JSON etc.
//!
//! # In scope:
//!
//! * First class HTTP/1.1 protocol implementation
//! * Indication of connection states (such as when a connection must be closed)
//! * transfer-encoding: chunked
//! * Redirect handling (building URI and amending requests)
//!
//! # Out of scope:
//!
//! * Opening/closing sockets
//! * TLS (https)
//! * Cookie jars
//! * Authorization
//! * Body data transformations (charset, compression etc)
//!
//! The project is run as a companion project to [ureq](https://crates.io/crates/ureq),
//! specifically the [ureq 3.x rewrite](https://github.com/algesten/ureq/pull/762)
//!
//! # The http crate
//!
//! hoot is based on the [http crate](https://crates.io/crates/http) - a unified HTTP API for Rust.
//!

#![forbid(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::uninlined_format_args)]
// #![deny(missing_docs)]

// Re-export the basis for this library.
pub use http;

mod error;
pub use error::Error;

mod chunk;
mod ext;
mod util;

mod body;
pub use body::BodyMode;

pub mod client;

pub mod parser;
