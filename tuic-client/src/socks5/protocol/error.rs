use super::Reply;
use std::io::{self, ErrorKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("unsupported version {0:#x}")]
    UnsupportedVersion(u8),
    #[error("unsupported command {0:#x}")]
    UnsupportedCommand(u8),
    #[error("address type {0:#x} not supported")]
    AddressTypeNotSupported(u8),
    #[error("address domain name must be UTF-8 encoding")]
    AddressDomainInvalidEncoding,
    #[error("{0}")]
    Reply(Reply),
}

impl Error {
    pub fn as_reply(&self) -> Reply {
        match self {
            Self::Io(err) => match err.kind() {
                ErrorKind::ConnectionRefused => Reply::ConnectionRefused,
                _ => Reply::GeneralFailure,
            },
            Self::UnsupportedCommand(..) => Reply::CommandNotSupported,
            Self::AddressTypeNotSupported(..) => Reply::AddressTypeNotSupported,
            Self::AddressDomainInvalidEncoding | Self::UnsupportedVersion(..) => {
                Reply::GeneralFailure
            }
            Self::Reply(r) => *r,
        }
    }
}

impl From<Reply> for Error {
    fn from(reply: Reply) -> Self {
        match reply {
            Reply::Succeeded => unreachable!(),
            Reply::CommandNotSupported => Self::UnsupportedCommand(0),
            Reply::AddressTypeNotSupported => Self::AddressTypeNotSupported(0),
            reply => Error::Reply(reply),
        }
    }
}
