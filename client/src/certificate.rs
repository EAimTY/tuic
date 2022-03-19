use rustls::{Certificate, RootCertStore};
use rustls_pemfile::Item;
use std::{
    fs::{self, File},
    io::{BufReader, Error as IoError},
};
use thiserror::Error;
use webpki::Error as WebpkiError;

pub fn load_certificates(path: &str) -> Result<RootCertStore, CertificateError> {
    let mut file = BufReader::new(File::open(path)?);
    let mut certs = RootCertStore::empty();

    while let Ok(Some(item)) = rustls_pemfile::read_one(&mut file) {
        if let Item::X509Certificate(cert) = item {
            certs.add(&Certificate(cert))?;
        }
    }

    if certs.is_empty() {
        certs.add(&Certificate(fs::read(path)?))?;
    }

    Ok(certs)
}

#[derive(Error, Debug)]
pub enum CertificateError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Validation(#[from] WebpkiError),
}
