use anyhow::Result;
use rustls::{Certificate, PrivateKey};
use std::fs;

pub fn load_certificate(path: &str) -> Result<Certificate> {
    let cert = fs::read(path)?;
    let cert = Certificate(cert);
    Ok(cert)
}

pub fn load_private_key(path: &str) -> Result<PrivateKey> {
    let key = fs::read(path)?;
    let key = PrivateKey(key);
    Ok(key)
}
