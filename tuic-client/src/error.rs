use quinn::{ConnectError, ConnectionError};
use rustls::Error as RustlsError;
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
    Model(#[from] ModelError),
    #[error("load native certificates error: {0}")]
    LoadNativeCerts(IoError),
    #[error(transparent)]
    Rustls(#[from] RustlsError),
    #[error("{0}: {1}")]
    Socket(&'static str, IoError),
    #[error("timeout establishing connection")]
    Timeout,
    #[error("cannot resolve the server name")]
    DnsResolve,
    #[error("received packet from an unexpected source")]
    WrongPacketSource,
    #[error("invalid socks5 authentication")]
    InvalidSocks5Auth,
}

impl From<ConnectionError> for Error {
    fn from(err: ConnectionError) -> Self {
        Self::Io(IoError::from(err))
    }
}
