mod connection;
mod stream;

pub use self::{
    connection::{Connecting, Connection, IncomingPackets, IncomingPacketsError},
    stream::Stream,
};

use crate::UdpRelayMode;
use quinn::{
    congestion::ControllerFactory, ApplicationClose, ClientConfig as QuinnClientConfig,
    ConnectError as QuinnConnectError, ConnectionClose, ConnectionError as QuinnConnectionError,
    Endpoint, EndpointConfig, NewConnection as QuinnNewConnection,
};
use quinn_proto::TransportError;
use rustls::{version, ClientConfig as RustlsClientConfig, RootCertStore};
use std::{
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
            Err(err) => return Err(ConnectError::from_quinn_connect_error(err)),
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
                Err(err) => return Err(ConnectError::from_quinn_connection_error(err)),
            }
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

#[derive(Clone, Debug)]
pub struct ClientConfig<C>
where
    C: ControllerFactory + Send + Sync + 'static,
{
    pub certs: RootCertStore,
    pub alpn_protocols: Vec<Vec<u8>>,
    pub disable_sni: bool,
    pub enable_0rtt: bool,
    pub udp_relay_mode: UdpRelayMode,
    pub congestion_controller: C,
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

impl ConnectError {
    #[inline]
    fn from_quinn_connect_error(err: QuinnConnectError) -> Self {
        match err {
            QuinnConnectError::UnsupportedVersion => Self::UnsupportedQUICVersion,
            QuinnConnectError::EndpointStopping => Self::EndpointStopping,
            QuinnConnectError::TooManyConnections => Self::TooManyConnections,
            QuinnConnectError::InvalidDnsName(err) => Self::InvalidDomainName(err),
            QuinnConnectError::InvalidRemoteAddress(err) => Self::InvalidRemoteAddress(err),
            QuinnConnectError::NoDefaultClientConfig => unreachable!(),
        }
    }

    #[inline]
    fn from_quinn_connection_error(err: QuinnConnectionError) -> Self {
        match err {
            QuinnConnectionError::VersionMismatch => Self::UnsupportedQUICVersion,
            QuinnConnectionError::TransportError(err) => Self::TransportError(err),
            QuinnConnectionError::ConnectionClosed(err) => Self::ConnectionClosed(err),
            QuinnConnectionError::ApplicationClosed(err) => Self::ApplicationClosed(err),
            QuinnConnectionError::Reset => Self::Reset,
            QuinnConnectionError::TimedOut => Self::TimedOut,
            QuinnConnectionError::LocallyClosed => Self::LocallyClosed,
        }
    }
}
