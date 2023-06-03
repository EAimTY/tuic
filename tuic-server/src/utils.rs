use rustls::{Certificate, PrivateKey};
use rustls_pemfile::Item;
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    fs::{self, File},
    io::{BufReader, Error as IoError},
    path::PathBuf,
    str::FromStr,
};

pub fn load_certs(path: PathBuf) -> Result<Vec<Certificate>, IoError> {
    let mut file = BufReader::new(File::open(&path)?);
    let mut certs = Vec::new();

    while let Ok(Some(item)) = rustls_pemfile::read_one(&mut file) {
        if let Item::X509Certificate(cert) = item {
            certs.push(Certificate(cert));
        }
    }

    if certs.is_empty() {
        certs = vec![Certificate(fs::read(&path)?)];
    }

    Ok(certs)
}

pub fn load_priv_key(path: PathBuf) -> Result<PrivateKey, IoError> {
    let mut file = BufReader::new(File::open(&path)?);
    let mut priv_key = None;

    while let Ok(Some(item)) = rustls_pemfile::read_one(&mut file) {
        if let Item::RSAKey(key) | Item::PKCS8Key(key) | Item::ECKey(key) = item {
            priv_key = Some(key);
        }
    }

    priv_key
        .map(Ok)
        .unwrap_or_else(|| fs::read(&path))
        .map(PrivateKey)
}

#[derive(Clone, Copy)]
pub enum UdpRelayMode {
    Native,
    Quic,
}

impl Display for UdpRelayMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Native => write!(f, "native"),
            Self::Quic => write!(f, "quic"),
        }
    }
}

pub enum CongestionControl {
    Cubic,
    NewReno,
    Bbr,
}

impl FromStr for CongestionControl {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("cubic") {
            Ok(Self::Cubic)
        } else if s.eq_ignore_ascii_case("new_reno") || s.eq_ignore_ascii_case("newreno") {
            Ok(Self::NewReno)
        } else if s.eq_ignore_ascii_case("bbr") {
            Ok(Self::Bbr)
        } else {
            Err("invalid congestion control")
        }
    }
}
