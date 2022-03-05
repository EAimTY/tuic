use std::io::Error as IoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error("unsupported version {0:#x}")]
    UnsupportedVersion(u8),
    #[error("unsupported command {0:#x}")]
    UnsupportedCommand(u8),
    #[error("unsupported address type {0:#x}")]
    UnsupportedAddressType(u8),
    #[error("unsupported reply {0:#x}")]
    UnsupportedReply(u8),
    #[error("address domain name must be in UTF-8")]
    AddressInvalidEncoding,
}
