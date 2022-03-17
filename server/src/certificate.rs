use rustls::{Certificate, PrivateKey};
use std::{fs, io::Error as IoError};
use thiserror::Error;

pub fn load_certificate(path: &str) -> Result<Certificate, CertificateError> {
    let cert = fs::read(path)?;
    let cert = Certificate(cert);
    Ok(cert)
}

pub fn load_private_key(path: &str) -> Result<PrivateKey, CertificateError> {
    let key = fs::read(path)?;
    let key = PrivateKey(key);
    Ok(key)
}

#[derive(Error, Debug)]
pub enum CertificateError {
    #[error(transparent)]
    Io(#[from] IoError),
}
