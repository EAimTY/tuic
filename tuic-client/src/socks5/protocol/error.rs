use std::io::{self, ErrorKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    IoError(#[from] io::Error),
    #[error("unsupported version {0:#x}")]
    UnsupportedVersion(u8),
    #[error("unsupported command {0:#x}")]
    UnsupportedCommand(u8),
    #[error("address type {0:#x} not supported")]
    AddressTypeNotSupported(u8),
    #[error("address domain name must be UTF-8 encoding")]
    AddressDomainInvalidEncoding,
}
