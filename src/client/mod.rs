mod connection;
mod incoming;
mod stream;

pub use self::{
    connection::{Connecting, Connection, ConnectionError},
    incoming::{IncomingPackets, IncomingPacketsError},
    stream::Stream,
};

use crate::{CongestionControl, UdpRelayMode};
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    ClientConfig as QuinnClientConfig, ConnectError as QuinnConnectError,
    ConnectionError as QuinnConnectionError, Endpoint, EndpointConfig,
    NewConnection as QuinnNewConnection,
};
use rustls::{version, ClientConfig as RustlsClientConfig, RootCertStore};
use std::{
    convert::Infallible,
    fmt::Debug,
    io::Error as IoError,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
};
use thiserror::Error;

pub struct Client {
    endpoint: Endpoint,
    enable_quic_0rtt: bool,
    udp_relay_mode: UdpRelayMode,
}

impl Client {
    pub fn bind(cfg: ClientConfig, socket: UdpSocket) -> Result<Self, ClientError> {
        let mut crypto = RustlsClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&version::TLS13])
            .unwrap()
            .with_root_certificates(cfg.certificates)
            .with_no_client_auth();

        crypto.alpn_protocols = cfg.alpn_protocols;
        crypto.enable_early_data = cfg.enable_quic_0rtt;
        crypto.enable_sni = !cfg.disable_sni;

        let mut quinn_config = QuinnClientConfig::new(Arc::new(crypto));

        let transport = Arc::get_mut(&mut quinn_config.transport).unwrap();
        transport.max_idle_timeout(None);

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

        let (mut ep, _) = Endpoint::new(EndpointConfig::default(), None, socket)?;
        ep.set_default_client_config(quinn_config);

        Ok(Self {
            endpoint: ep,
            udp_relay_mode: cfg.udp_relay_mode,
            enable_quic_0rtt: cfg.enable_quic_0rtt,
        })
    }

    pub fn reconfigure(&mut self, cfg: ClientConfig) -> Result<(), Infallible> {
        let mut crypto = RustlsClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&version::TLS13])
            .unwrap()
            .with_root_certificates(cfg.certificates)
            .with_no_client_auth();

        crypto.alpn_protocols = cfg.alpn_protocols;
        crypto.enable_early_data = cfg.enable_quic_0rtt;
        crypto.enable_sni = !cfg.disable_sni;

        let mut quinn_config = QuinnClientConfig::new(Arc::new(crypto));

        let transport = Arc::get_mut(&mut quinn_config.transport).unwrap();
        transport.max_idle_timeout(None);

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

        self.endpoint.set_default_client_config(quinn_config);

        self.udp_relay_mode = cfg.udp_relay_mode;
        self.enable_quic_0rtt = cfg.enable_quic_0rtt;

        Ok(())
    }

    pub fn rebind(&mut self, socket: UdpSocket) -> Result<(), ClientError> {
        self.endpoint.rebind(socket)?;
        Ok(())
    }

    pub async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
        token: [u8; 32],
    ) -> Result<(Connection, IncomingPackets), ClientError> {
        let conn = self
            .endpoint
            .connect(addr, server_name)
            .map_err(ClientError::from_quinn_connect_error)?;

        let QuinnNewConnection {
            connection,
            datagrams,
            uni_streams,
            ..
        } = if self.enable_quic_0rtt {
            match conn.into_0rtt() {
                Ok((conn, _)) => conn,
                Err(conn) => {
                    return Err(ClientError::Convert0Rtt(Connecting::new(
                        conn,
                        token,
                        self.udp_relay_mode,
                    )))
                }
            }
        } else {
            conn.await
                .map_err(ClientError::from_quinn_connection_error)?
        };

        Ok(Connection::new(
            connection,
            uni_streams,
            datagrams,
            token,
            self.udp_relay_mode,
        ))
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("endpoint", &self.endpoint)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub certificates: RootCertStore,
    pub alpn_protocols: Vec<Vec<u8>>,
    pub disable_sni: bool,
    pub enable_quic_0rtt: bool,
    pub udp_relay_mode: UdpRelayMode,
    pub congestion_controller: CongestionControl,
}

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("failed to convert QUIC connection into 0-RTT")]
    Convert0Rtt(Connecting),
    #[error(transparent)]
    Io(#[from] IoError),
    #[error("endpoint stopping")]
    EndpointStopping,
    #[error("too many connections")]
    TooManyConnections,
    #[error("invalid DNS name: {0}")]
    InvalidDnsName(String),
    #[error("invalid remote address: {0}")]
    InvalidRemoteAddress(SocketAddr),
    #[error("unsupported QUIC version")]
    UnsupportedQUICVersion,
}

impl ClientError {
    #[inline]
    fn from_quinn_connect_error(err: QuinnConnectError) -> Self {
        match err {
            QuinnConnectError::UnsupportedVersion => Self::UnsupportedQUICVersion,
            QuinnConnectError::EndpointStopping => Self::EndpointStopping,
            QuinnConnectError::TooManyConnections => Self::TooManyConnections,
            QuinnConnectError::InvalidDnsName(err) => Self::InvalidDnsName(err),
            QuinnConnectError::InvalidRemoteAddress(err) => Self::InvalidRemoteAddress(err),
            QuinnConnectError::NoDefaultClientConfig => unreachable!(),
        }
    }

    #[inline]
    fn from_quinn_connection_error(err: QuinnConnectionError) -> Self {
        Self::from(IoError::from(err))
    }
}
