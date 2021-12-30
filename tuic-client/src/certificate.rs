use rustls::Certificate;
use thiserror::Error;

const CERT: &[u8] = include_bytes!("../../cert.der");

pub fn load_cert() -> Result<Certificate, CertificateError> {
    let cert = Certificate(CERT.to_vec());
    Ok(cert)
}

#[derive(Debug, Error)]
pub enum CertificateError {}
