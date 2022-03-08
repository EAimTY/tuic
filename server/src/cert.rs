use anyhow::Result;
use rustls::{Certificate, PrivateKey};
use std::fs;

pub fn load_cert(cert_path: &str) -> Result<Certificate> {
    let cert = fs::read(cert_path)?;
    let cert = Certificate(cert);
    Ok(cert)
}

pub fn load_priv_key(key_path: &str) -> Result<PrivateKey> {
    let key = fs::read(key_path)?;
    let key = PrivateKey(key);
    Ok(key)
}
