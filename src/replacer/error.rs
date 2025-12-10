use glob;
use std::any::Any;
use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("end of iteration")]
    EndOfIteration,
    #[error("bad pattern: {0}")]
    BadPattern(String),
    #[error("cannot handle path: {0}")]
    PathError(String),
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Glob error: {0}")]
    GlobError(#[from] glob::GlobError),
    #[error("Pattern error: {0}")]
    PatternError(#[from] glob::PatternError),
    #[error("internal error: {0}")]
    Internal(#[from] Box<Error>),
    #[error("thread panic: {0}")]
    ThreadPanic(String),
}

impl From<Box<dyn Any + Send + 'static>> for Error {
    fn from(panic: Box<(dyn Any + Send + 'static)>) -> Self {
        if let Some(s) = panic.downcast_ref::<String>() {
            return Error::ThreadPanic(s.clone());
        }

        if let Some(s) = panic.downcast_ref::<&'static str>() {
            return Error::ThreadPanic(s.to_string());
        }

        // If the panic was caused by an unknown/unprintable type, return a generic message
        Error::ThreadPanic("A thread panicked with an unprintable payload.".to_string())
    }
}
