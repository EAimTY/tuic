mod connection;

pub use self::connection::{Connecting, Connection, IncomingPackets};

use crate::UdpRelayMode;
use quinn::{
    congestion::ControllerFactory, ApplicationClose, ClientConfig as QuinnClientConfig,
    ConnectError as QuinnConnectError, ConnectionClose, ConnectionError as QuinnConnectionError,
    Endpoint, EndpointConfig, NewConnection as QuinnNewConnection,
};
use quinn_proto::TransportError;
use rustls::{version, ClientConfig as RustlsClientConfig, RootCertStore};
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    io::Result as IoResult,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    sync::Arc,
};
use thiserror::Error;

pub struct Client {
    endpoint: Endpoint,
    enable_0rtt: bool,
    udp_relay_mode: UdpRelayMode,
}

impl Client {
    pub fn bind<C>(cfg: ClientConfig<C>, addr: impl ToSocketAddrs) -> IoResult<Self>
    where
        C: ControllerFactory + Send + Sync + 'static,
    {
        let socket = UdpSocket::bind(addr)?;
        let (mut ep, _) = Endpoint::new(EndpointConfig::default(), None, socket)?;

        let mut crypto = RustlsClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&version::TLS13])
            .unwrap()
            .with_root_certificates(cfg.certs)
            .with_no_client_auth();

        crypto.alpn_protocols = cfg.alpn_protocols;
        crypto.enable_early_data = cfg.enable_0rtt;
        crypto.enable_sni = !cfg.disable_sni;

        let mut quinn_config = QuinnClientConfig::new(Arc::new(crypto));

        let transport = Arc::get_mut(&mut quinn_config.transport).unwrap();
        transport.congestion_controller_factory(cfg.congestion_controller);
        transport.max_idle_timeout(None);

        ep.set_default_client_config(quinn_config);

        Ok(Self {
            endpoint: ep,
            udp_relay_mode: cfg.udp_relay_mode,
            enable_0rtt: cfg.enable_0rtt,
        })
    }

    pub fn reconfigure<C>(&mut self, cfg: ClientConfig<C>)
    where
        C: ControllerFactory + Send + Sync + 'static,
    {
        let mut crypto = RustlsClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&version::TLS13])
            .unwrap()
            .with_root_certificates(cfg.certs)
            .with_no_client_auth();

        crypto.alpn_protocols = cfg.alpn_protocols;
        crypto.enable_early_data = cfg.enable_0rtt;
        crypto.enable_sni = !cfg.disable_sni;

        let mut quinn_config = QuinnClientConfig::new(Arc::new(crypto));

        let transport = Arc::get_mut(&mut quinn_config.transport).unwrap();
        transport.congestion_controller_factory(cfg.congestion_controller);
        transport.max_idle_timeout(None);

        self.endpoint.set_default_client_config(quinn_config);

        self.udp_relay_mode = cfg.udp_relay_mode;
        self.enable_0rtt = cfg.enable_0rtt;
    }

    pub fn rebind(&mut self, addr: impl ToSocketAddrs) -> IoResult<()> {
        let socket = UdpSocket::bind(addr)?;
        self.endpoint.rebind(socket)
    }

    pub async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
        token: [u8; 32],
    ) -> Result<(Connection, IncomingPackets), ConnectError> {
        let conn = match self.endpoint.connect(addr, server_name) {
            Ok(conn) => conn,
            Err(err) => {
                return Err(match err {
                    QuinnConnectError::UnsupportedVersion => ConnectError::UnsupportedQUICVersion,
                    QuinnConnectError::EndpointStopping => ConnectError::EndpointStopping,
                    QuinnConnectError::TooManyConnections => ConnectError::TooManyConnections,
                    QuinnConnectError::InvalidDnsName(err) => ConnectError::InvalidDomainName(err),
                    QuinnConnectError::InvalidRemoteAddress(err) => {
                        ConnectError::InvalidRemoteAddress(err)
                    }
                    QuinnConnectError::NoDefaultClientConfig => unreachable!(),
                })
            }
        };

        let QuinnNewConnection {
            connection,
            datagrams,
            uni_streams,
            ..
        } = if self.enable_0rtt {
            match conn.into_0rtt() {
                Ok((conn, _)) => conn,
                Err(conn) => {
                    return Err(ConnectError::Convert0Rtt(Connecting::new(
                        conn,
                        token,
                        self.udp_relay_mode,
                    )))
                }
            }
        } else {
            match conn.await {
                Ok(conn) => conn,
                Err(err) => {
                    return Err(match err {
                        QuinnConnectionError::VersionMismatch => {
                            ConnectError::UnsupportedQUICVersion
                        }
                        QuinnConnectionError::TransportError(err) => {
                            ConnectError::TransportError(err)
                        }
                        QuinnConnectionError::ConnectionClosed(err) => {
                            ConnectError::ConnectionClosed(err)
                        }
                        QuinnConnectionError::ApplicationClosed(err) => {
                            ConnectError::ApplicationClosed(err)
                        }
                        QuinnConnectionError::Reset => ConnectError::Reset,
                        QuinnConnectionError::TimedOut => ConnectError::TimedOut,
                        QuinnConnectionError::LocallyClosed => ConnectError::LocallyClosed,
                    })
                }
            }
        };

        let conn = Connection::new(connection, token, self.udp_relay_mode);
        let pkts = IncomingPackets::new(uni_streams, datagrams, self.udp_relay_mode);

        Ok((conn, pkts))
    }
}

#[derive(Clone, Debug)]
pub struct ClientConfig<C> {
    pub certs: RootCertStore,
    pub alpn_protocols: Vec<Vec<u8>>,
    pub disable_sni: bool,
    pub enable_0rtt: bool,
    pub udp_relay_mode: UdpRelayMode,
    pub congestion_controller: C,
}

#[derive(Clone, Debug)]
pub enum ServerAddr {
    SocketAddr { addr: SocketAddr, name: String },
    DomainAddr { domain: String, port: u16 },
}

impl Display for ServerAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            ServerAddr::SocketAddr { addr, name } => write!(f, "{addr} ({name})"),
            ServerAddr::DomainAddr { domain, port } => write!(f, "{domain}:{port}"),
        }
    }
}

#[derive(Error, Debug)]
pub enum ConnectError {
    #[error("failed to convert QUIC connection into 0-RTT")]
    Convert0Rtt(Connecting),
    #[error("unsupported QUIC version")]
    UnsupportedQUICVersion,
    #[error("endpoint stopping")]
    EndpointStopping,
    #[error("too many connections")]
    TooManyConnections,
    #[error("invalid domain name: {0}")]
    InvalidDomainName(String),
    #[error("invalid remote address: {0}")]
    InvalidRemoteAddress(SocketAddr),
    #[error(transparent)]
    TransportError(#[from] TransportError),
    #[error("aborted by peer: {0}")]
    ConnectionClosed(ConnectionClose),
    #[error("closed by peer: {0}")]
    ApplicationClosed(ApplicationClose),
    #[error("reset by peer")]
    Reset,
    #[error("timed out")]
    TimedOut,
    #[error("closed")]
    LocallyClosed,
}
