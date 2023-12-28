//! no_std, allocation free http library.

// For tests we use std.
// #![cfg_attr(not(test), no_std)]

mod chunk;
use chunk::Dechunker;

mod out;

mod util;
use util::LengthChecker;

mod vars;

mod parser;

mod error;
pub use error::HootError;
pub(crate) use error::Result;

pub mod client;

pub mod server;

mod header;
pub use header::Header;

mod body;
pub use body::BodyPart;
use body::RecvBodyMode;

mod url;
pub use url::Url;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
}

impl From<u8> for HttpVersion {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Http10,
            1 => Self::Http11,
            _ => panic!("Unknown HTTP version"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Method {
    OPTIONS,
    GET,
    POST,
    PUT,
    DELETE,
    HEAD,
    TRACE,
    CONNECT,
    PATCH,
}

impl Method {
    pub fn has_request_body(&self) -> bool {
        use Method::*;
        matches!(self, POST | PUT | PATCH)
    }
}

impl TryFrom<&str> for Method {
    type Error = HootError;

    fn try_from(value: &str) -> core::prelude::v1::Result<Self, Self::Error> {
        match value {
            "OPTIONS" => Ok(Method::OPTIONS),
            "GET" => Ok(Method::GET),
            "POST" => Ok(Method::POST),
            "PUT" => Ok(Method::PUT),
            "DELETE" => Ok(Method::DELETE),
            "HEAD" => Ok(Method::HEAD),
            "TRACE" => Ok(Method::TRACE),
            "CONNECT" => Ok(Method::CONNECT),
            "PATCH" => Ok(Method::PATCH),
            _ => Err(HootError::UnknownMethod),
        }
    }
}

#[derive(Default)]
pub(crate) struct CallState {
    pub version: Option<HttpVersion>,
    pub method: Option<Method>,
    pub send_checker: Option<LengthChecker>,
    pub recv_body_mode: Option<RecvBodyMode>,
    pub recv_checker: Option<LengthChecker>,
    pub dechunker: Option<Dechunker>,
    pub did_read_to_end: bool,
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

    impl fmt::Debug for Method {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::OPTIONS => write!(f, "OPTIONS"),
                Self::GET => write!(f, "GET"),
                Self::POST => write!(f, "POST"),
                Self::PUT => write!(f, "PUT"),
                Self::DELETE => write!(f, "DELETE"),
                Self::HEAD => write!(f, "HEAD"),
                Self::TRACE => write!(f, "TRACE"),
                Self::CONNECT => write!(f, "CONNECT"),
                Self::PATCH => write!(f, "PATCH"),
            }
        }
    }
}
