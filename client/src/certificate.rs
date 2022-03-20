use rustls::Certificate;
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
