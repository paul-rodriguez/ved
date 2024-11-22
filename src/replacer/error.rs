use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("end of iteration")]
    EndOfIteration,
    #[error("bad pattern: {0}")]
    BadPattern(String),
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("internal error: {0}")]
    Internal(#[from] Box<Error>),
}
