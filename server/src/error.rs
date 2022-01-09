use crate::certificate::{CertificateError, PrivateKeyError};
use rustls::Error as RustlsError;
use std::io::Error as IoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error(transparent)]
    PrivateKey(#[from] PrivateKeyError),
    #[error(transparent)]
    Rustls(#[from] RustlsError),
    #[error(transparent)]
    Io(#[from] IoError),
}
