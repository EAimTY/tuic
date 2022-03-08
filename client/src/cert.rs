use anyhow::Result;
use rustls::Certificate;
use std::fs;

pub fn load_cert(cert_path: &str) -> Result<Certificate> {
    let cert = fs::read(cert_path)?;
    let cert = Certificate(cert);
    Ok(cert)
}
