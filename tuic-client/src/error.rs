use crate::certificate;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Certificate(#[from] certificate::CertificateError),
    #[error("Failed to create the client endpoint")]
    Endpoint(#[from] io::Error),
    #[error("Failed to get connection from the pool")]
    GetConnection,
    #[error("TUIC Authentication failed")]
    AuthFailed,
    #[error("Socks5 Authentication failed")]
    Socks5AuthFailed,
}
