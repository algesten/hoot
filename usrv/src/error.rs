use std::io;
use std::string::FromUtf8Error;

use hoot::HootError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Hoot(#[from] HootError),

    #[error("{0}")]
    Io(#[from] io::Error),

    #[error("ut8: {0}")]
    Utf8(#[from] FromUtf8Error),
}
