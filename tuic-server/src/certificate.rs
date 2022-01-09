use rustls::{Certificate, PrivateKey};
use std::{fs, io::Error as IoError};
use thiserror::Error;

pub fn load_cert(cert_path: &str) -> Result<Certificate, CertificateError> {
    let cert = fs::read(cert_path)?;
    let cert = Certificate(cert);
    Ok(cert)
}

pub fn load_priv_key(key_path: &str) -> Result<PrivateKey, PrivateKeyError> {
    let key = fs::read(key_path)?;
    let key = PrivateKey(key);
    Ok(key)
}

#[derive(Debug, Error)]
pub enum CertificateError {
    #[error("Failed to read the certificate file: {0}")]
    Read(#[from] IoError),
}

#[derive(Debug, Error)]
pub enum PrivateKeyError {
    #[error("Failed to read the private key file: {0}")]
    Read(#[from] IoError),
}
