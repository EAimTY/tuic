use crate::certificate::CertificateError;
use std::io::Error as IoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error("Failed to create the client endpoint")]
    Endpoint(#[from] IoError),
    #[error("Failed to get connection from the pool")]
    GetConnection,
    #[error("TUIC Authentication failed")]
    AuthFailed,
    #[error("Socks5 Authentication failed")]
    Socks5AuthFailed,
}
