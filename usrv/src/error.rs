use std::io;

use hoot::HootError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Hoot(#[from] HootError),

    #[error("{0}")]
    Io(#[from] io::Error),
}
