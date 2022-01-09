use crate::certificate::CertificateError;
use std::io::Error as IoError;
use thiserror::Error;
use webpki::Error as WebPkiError;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    WebPki(#[from] WebPkiError),
}
