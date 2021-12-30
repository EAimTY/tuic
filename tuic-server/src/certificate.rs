use rustls::{Certificate, PrivateKey};
use thiserror::Error;

const CERT: &[u8] = include_bytes!("../../cert.der");
const KEY: &[u8] = include_bytes!("../../key.der");

pub fn load_cert() -> Result<Certificate, CertificateError> {
    let cert = Certificate(CERT.to_vec());
    Ok(cert)
}

pub fn load_priv_key() -> Result<PrivateKey, PrivateKeyError> {
    let priv_key = PrivateKey(KEY.to_vec());
    Ok(priv_key)
}

#[derive(Debug, Error)]
pub enum CertificateError {}

#[derive(Debug, Error)]
pub enum PrivateKeyError {}
