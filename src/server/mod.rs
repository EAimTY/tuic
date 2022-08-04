mod connection;
mod incoming;

pub use self::connection::Connecting;

use crate::CongestionControl;
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    Endpoint, EndpointConfig, IdleTimeout, Incoming, ServerConfig as QuinnServerConfig, VarInt,
};
use rustls::{
    version, Certificate, Error as RustlsError, PrivateKey, ServerConfig as RustlsServerConfig,
};
use std::{io::Error as IoError, net::UdpSocket, sync::Arc, time::Duration};
use thiserror::Error;

#[derive(Debug)]
pub struct Server {
    endpoint: Endpoint,
    incoming: Incoming,
}

impl Server {
    pub fn bind(cfg: ServerConfig, socket: UdpSocket) -> Result<Self, ServerError> {
        let mut crypto = RustlsServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(cfg.certificate_chain, cfg.private_key)?;

        if cfg.allow_quic_0rtt {
            crypto.max_early_data_size = u32::MAX;
        }

        crypto.alpn_protocols = cfg.alpn_protocols;

        let mut quinn_config = QuinnServerConfig::with_crypto(Arc::new(crypto));
        let transport = Arc::get_mut(&mut quinn_config.transport).unwrap();

        let max_idle_timeout = cfg.max_idle_timeout.map(|timeout| {
            IdleTimeout::try_from(timeout).unwrap_or_else(|_| IdleTimeout::from(VarInt::MAX))
        });

        transport.max_idle_timeout(max_idle_timeout);

        match cfg.congestion_controller {
            CongestionControl::Cubic => {
                transport.congestion_controller_factory(Arc::new(CubicConfig::default()));
            }
            CongestionControl::NewReno => {
                transport.congestion_controller_factory(Arc::new(NewRenoConfig::default()));
            }
            CongestionControl::Bbr => {
                transport.congestion_controller_factory(Arc::new(BbrConfig::default()));
            }
        }

        let (endpoint, incoming) =
            Endpoint::new(EndpointConfig::default(), Some(quinn_config), socket)?;

        Ok(Self { endpoint, incoming })
    }

    pub fn reconfigure(&mut self, cfg: ServerConfig) -> Result<(), ServerError> {
        let mut crypto = RustlsServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(cfg.certificate_chain, cfg.private_key)?;

        if cfg.allow_quic_0rtt {
            crypto.max_early_data_size = u32::MAX;
        }

        crypto.alpn_protocols = cfg.alpn_protocols;

        let mut quinn_config = QuinnServerConfig::with_crypto(Arc::new(crypto));
        let transport = Arc::get_mut(&mut quinn_config.transport).unwrap();

        let max_idle_timeout = cfg.max_idle_timeout.map(|timeout| {
            IdleTimeout::try_from(timeout).unwrap_or_else(|_| IdleTimeout::from(VarInt::MAX))
        });

        transport.max_idle_timeout(max_idle_timeout);

        match cfg.congestion_controller {
            CongestionControl::Cubic => {
                transport.congestion_controller_factory(Arc::new(CubicConfig::default()));
            }
            CongestionControl::NewReno => {
                transport.congestion_controller_factory(Arc::new(NewRenoConfig::default()));
            }
            CongestionControl::Bbr => {
                transport.congestion_controller_factory(Arc::new(BbrConfig::default()));
            }
        }

        self.endpoint.set_server_config(Some(quinn_config));

        Ok(())
    }

    pub fn rebind(&mut self, socket: UdpSocket) -> Result<(), ServerError> {
        self.endpoint.rebind(socket)?;
        Ok(())
    }

    pub async fn accept(&self) -> Connecting {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub certificate_chain: Vec<Certificate>,
    pub private_key: PrivateKey,
    pub alpn_protocols: Vec<Vec<u8>>,
    pub allow_quic_0rtt: bool,
    pub max_idle_timeout: Option<Duration>,
    pub congestion_controller: CongestionControl,
}

#[derive(Error, Debug)]
pub enum ServerError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Certificate(#[from] RustlsError),
}
