use anyhow::Result;
use rustls::Certificate;
use std::fs;

pub fn load_certificate(path: &str) -> Result<Certificate> {
    let cert = fs::read(path)?;
    let cert = Certificate(cert);
    Ok(cert)
}
