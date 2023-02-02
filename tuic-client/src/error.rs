use quinn::{ConnectError, ConnectionError};
use std::io::Error as IoError;
use thiserror::Error;
use tuic_quinn::Error as ModelError;
use webpki::Error as WebpkiError;

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
    #[error(transparent)]
    Webpki(#[from] WebpkiError),
    #[error("timeout establishing connection")]
    Timeout,
    #[error("cannot resolve the server name")]
    DnsResolve,
    #[error("invalid socks5 authentication")]
    InvalidSocks5Auth,
}
