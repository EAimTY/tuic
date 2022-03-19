use rustls::Certificate;
use rustls_pemfile::Item;
use std::{
    fs::{self, File},
    io::{BufReader, Error as IoError},
};
use thiserror::Error;

pub fn load_certificates(path: &str) -> Result<Vec<Certificate>, CertificateError> {
    let file = fs::read(path)?;

    if der_parser::parse_der(&file).is_ok() {
        return Ok(vec![Certificate(file)]);
    }

    let mut file = BufReader::new(File::open(path)?);
    let mut certs = Vec::new();

    while let Some(item) = rustls_pemfile::read_one(&mut file)? {
        if let Item::X509Certificate(cert) = item {
            certs.push(Certificate(cert));
        }
    }

    if !certs.is_empty() {
        Ok(certs)
    } else {
        Err(CertificateError::NoCertificateFound)
    }
}

#[derive(Error, Debug)]
pub enum CertificateError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error("No valid certificate found in file")]
    NoCertificateFound,
}
