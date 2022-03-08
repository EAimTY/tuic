use std::io::Error as IoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] IoError),
    #[error("unsupported socks5 version {0:#x}")]
    UnsupportedSocks5Version(u8),
    #[error("unsupported socks5 password authentication version {0:#x}")]
    UnsupportedPasswordAuthenticationVersion(u8),
    #[error("unsupported command {0:#x}")]
    UnsupportedCommand(u8),
    #[error("unsupported address type {0:#x}")]
    UnsupportedAddressType(u8),
    #[error("address domain name must be in UTF-8")]
    AddressInvalidEncoding,
}
