//! no_std, allocation free http library.

// For tests we use std.
#![cfg_attr(not(test), no_std)]

mod chunk;

mod out;
mod util;

mod vars;

mod parser;

mod error;
pub use error::HootError;
pub(crate) use error::Result;

pub mod client;

mod header;
pub use header::Header;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
}

#[cfg(any(std, test))]
mod std_impls {
    use super::*;
    use std::fmt;

    impl fmt::Debug for HttpVersion {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Http10 => write!(f, "Http10"),
                Self::Http11 => write!(f, "Http11"),
            }
        }
    }
}
