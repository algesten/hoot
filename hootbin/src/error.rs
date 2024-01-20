use std::io;
use std::str::Utf8Error;

use hoot::{HootError, UrlError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] io::Error),

    #[error("{0}")]
    Hoot(#[from] HootError),

    #[error("utf8: {0}")]
    Utf8(#[from] Utf8Error),

    #[error("unhandled method")]
    UnhandledMethod,

    #[error("url: {0}")]
    UrlError(#[from] UrlError),
}

impl From<Error> for io::Error {
    fn from(value: Error) -> Self {
        if let Error::Io(e) = value {
            return e;
        } else {
            let s = value.to_string();
            io::Error::new(io::ErrorKind::Other, s)
        }
    }
}
