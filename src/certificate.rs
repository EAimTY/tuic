use anyhow::Result;
use rustls::{Certificate, PrivateKey};

const CERT: &[u8] = include_bytes!("../cert.der");
const KEY: &[u8] = include_bytes!("../key.der");

pub fn load_cert() -> Result<Certificate> {
    let cert = Certificate(CERT.to_vec());
    Ok(cert)
}

pub fn load_priv_key() -> Result<PrivateKey> {
    let priv_key = PrivateKey(KEY.to_vec());
    Ok(priv_key)
}
