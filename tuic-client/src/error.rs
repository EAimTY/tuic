use crate::certificate::CertificateError;
use std::io::Error as IoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error(transparent)]
    Io(#[from] IoError),
}
