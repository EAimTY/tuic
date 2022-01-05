use super::Reply;
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
    #[error("unsupported socks version {0:#x}")]
    UnsupportedSocksVersion(u8),
    #[error("unsupported command {0:#x}")]
    UnsupportedCommand(u8),
    #[error("{0}")]
    Reply(Reply),
}

impl From<Error> for io::Error {
    fn from(err: Error) -> io::Error {
        match err {
            Error::IoError(err) => err,
            e => io::Error::new(ErrorKind::Other, e),
        }
    }
}

impl Error {
    pub fn as_reply(&self) -> Reply {
        match self {
            Self::IoError(err) => match err.kind() {
                ErrorKind::ConnectionRefused => Reply::ConnectionRefused,
                _ => Reply::GeneralFailure,
            },
            Self::AddressTypeNotSupported(..) => Reply::AddressTypeNotSupported,
            Self::AddressDomainInvalidEncoding => Reply::GeneralFailure,
            Self::UnsupportedSocksVersion(..) => Reply::GeneralFailure,
            Self::UnsupportedCommand(..) => Reply::CommandNotSupported,
            Self::Reply(r) => *r,
        }
    }
}
