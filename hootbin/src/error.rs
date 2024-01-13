use std::io;
use std::str::Utf8Error;

use hoot::HootError;
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
}
