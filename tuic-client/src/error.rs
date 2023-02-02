use quinn::{ConnectError, ConnectionError};
use std::io::Error as IoError;
use thiserror::Error;
use tuic_quinn::Error as ModelError;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Connect(#[from] ConnectError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("timeout")]
    Timeout,
    #[error("invalid authentication")]
    InvalidAuth,
}
