use rustls::{Certificate, PrivateKey};
use rustls_pemfile::Item;
use std::{
    fs::{self, File},
    io::{BufReader, Error as IoError},
};

pub fn load_certificates(path: &str) -> Result<Vec<Certificate>, IoError> {
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

pub fn load_private_key(path: &str) -> Result<PrivateKey, IoError> {
    let mut file = BufReader::new(File::open(path)?);
    let mut priv_key = None;

    while let Ok(Some(item)) = rustls_pemfile::read_one(&mut file) {
        if let Item::RSAKey(key) | Item::PKCS8Key(key) | Item::ECKey(key) = item {
            priv_key = Some(key);
        }
    }

    priv_key
        .map(Ok)
        .unwrap_or_else(|| fs::read(path))
        .map(PrivateKey)
}
