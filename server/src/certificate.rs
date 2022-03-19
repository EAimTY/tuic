use rustls::{Certificate, PrivateKey};
use rustls_pemfile::Item;
use std::{
    fs::{self, File},
    io::{BufReader, Error as IoError},
};
use thiserror::Error;

pub fn load_certificates(path: &str) -> Result<Vec<Certificate>, CertificateError> {
    let mut file = BufReader::new(File::open(path)?);
    let mut certs = Vec::new();

    while let Ok(Some(item)) = rustls_pemfile::read_one(&mut file) {
        if let Item::X509Certificate(cert) = item {
            certs.push(Certificate(cert));
        }
    }

    if certs.is_empty() {
        certs = vec![Certificate(fs::read(path)?)];
    }

    Ok(certs)
}

pub fn load_private_key(path: &str) -> Result<PrivateKey, CertificateError> {
    let mut file = BufReader::new(File::open(path)?);
    let mut priv_key = None;

    while let Ok(Some(item)) = rustls_pemfile::read_one(&mut file) {
        if let Item::RSAKey(key) | Item::PKCS8Key(key) = item {
            priv_key = Some(PrivateKey(key));
        }
    }

    if priv_key.is_none() {
        priv_key = Some(PrivateKey(fs::read(path)?));
    }

    let priv_key = unsafe { priv_key.unwrap_unchecked() };

    Ok(priv_key)
}

#[derive(Error, Debug)]
pub enum CertificateError {
    #[error(transparent)]
    Io(#[from] IoError),
}
