use rustls::Certificate;
use std::{fs, io::Error as IoError};
use thiserror::Error;

pub fn load_cert(cert_path: &str) -> Result<Certificate, CertificateError> {
    let cert = fs::read(cert_path)?;
    let cert = Certificate(cert);
    Ok(cert)
}

#[derive(Debug, Error)]
pub enum CertificateError {
    #[error("Failed to read the certificate file: {0}")]
    Read(#[from] IoError),
}
