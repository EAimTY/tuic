use std::io::{self, ErrorKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    IoError(#[from] io::Error),
    #[error("address type {0:#x} not supported")]
    AddressTypeNotSupported(u8),
    #[error("address domain name must be UTF-8 encoding")]
    AddressDomainInvalidEncoding,
    #[error("unsupported TUIC protocol version {0:#x}")]
    UnsupportedTuicVersion(u8),
    #[error("unsupported command {0:#x}")]
    UnsupportedCommand(u8),
}

impl From<Error> for io::Error {
    fn from(err: Error) -> io::Error {
        match err {
            Error::IoError(err) => err,
            e => io::Error::new(ErrorKind::Other, e),
        }
    }
}
