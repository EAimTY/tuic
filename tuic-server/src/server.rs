use crate::{
    config::Config,
    connection::{Connection, DEFAULT_CONCURRENT_STREAMS},
    error::Error,
    utils::{self, CongestionControl},
};
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    Endpoint, EndpointConfig, IdleTimeout, ServerConfig, TokioRuntime, TransportConfig, VarInt,
};
use rustls::{version, ServerConfig as RustlsServerConfig};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket as StdUdpSocket},
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

pub struct Server {
    ep: Endpoint,
    users: Arc<HashMap<Uuid, Box<[u8]>>>,
    udp_relay_ipv6: bool,
    zero_rtt_handshake: bool,
    auth_timeout: Duration,
    task_negotiation_timeout: Duration,
    max_external_pkt_size: usize,
    gc_interval: Duration,
    gc_lifetime: Duration,
}

impl Server {
    pub fn init(cfg: Config) -> Result<Self, Error> {
        let certs = utils::load_certs(cfg.certificate)?;
        let priv_key = utils::load_priv_key(cfg.private_key)?;

        let mut crypto = RustlsServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(certs, priv_key)?;

        crypto.alpn_protocols = cfg.alpn;
        crypto.max_early_data_size = u32::MAX;
        crypto.send_half_rtt_data = cfg.zero_rtt_handshake;

        let mut config = ServerConfig::with_crypto(Arc::new(crypto));
        let mut tp_cfg = TransportConfig::default();

        tp_cfg
            .max_concurrent_bidi_streams(VarInt::from(DEFAULT_CONCURRENT_STREAMS))
            .max_concurrent_uni_streams(VarInt::from(DEFAULT_CONCURRENT_STREAMS))
            .send_window(cfg.send_window)
            .stream_receive_window(VarInt::from_u32(cfg.receive_window))
            .max_idle_timeout(Some(
                IdleTimeout::try_from(cfg.max_idle_time).map_err(|_| Error::InvalidMaxIdleTime)?,
            ));

        match cfg.congestion_control {
            CongestionControl::Cubic => {
                tp_cfg.congestion_controller_factory(Arc::new(CubicConfig::default()))
            }
            CongestionControl::NewReno => {
                tp_cfg.congestion_controller_factory(Arc::new(NewRenoConfig::default()))
            }
            CongestionControl::Bbr => {
                tp_cfg.congestion_controller_factory(Arc::new(BbrConfig::default()))
            }
        };

        config.transport_config(Arc::new(tp_cfg));

        let socket = {
            let domain = match cfg.server {
                SocketAddr::V4(_) => Domain::IPV4,
                SocketAddr::V6(_) => Domain::IPV6,
            };

            let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
                .map_err(|err| Error::Socket("failed to create endpoint UDP socket", err))?;

            if let Some(dual_stack) = cfg.dual_stack {
                socket.set_only_v6(!dual_stack).map_err(|err| {
                    Error::Socket("endpoint dual-stack socket setting error", err)
                })?;
            }

            socket
                .bind(&SockAddr::from(cfg.server))
                .map_err(|err| Error::Socket("failed to bind endpoint UDP socket", err))?;

            StdUdpSocket::from(socket)
        };

        let ep = Endpoint::new(
            EndpointConfig::default(),
            Some(config),
            socket,
            Arc::new(TokioRuntime),
        )?;

        Ok(Self {
            ep,
            users: Arc::new(cfg.users),
            udp_relay_ipv6: cfg.udp_relay_ipv6,
            zero_rtt_handshake: cfg.zero_rtt_handshake,
            auth_timeout: cfg.auth_timeout,
            task_negotiation_timeout: cfg.task_negotiation_timeout,
            max_external_pkt_size: cfg.max_external_packet_size,
            gc_interval: cfg.gc_interval,
            gc_lifetime: cfg.gc_lifetime,
        })
    }

    pub async fn start(&self) {
        log::warn!(
            "server started, listening on {}",
            self.ep.local_addr().unwrap()
        );

        loop {
            let Some(conn) = self.ep.accept().await else {
                return;
            };

            tokio::spawn(Connection::handle(
                conn,
                self.users.clone(),
                self.udp_relay_ipv6,
                self.zero_rtt_handshake,
                self.auth_timeout,
                self.task_negotiation_timeout,
                self.max_external_pkt_size,
                self.gc_interval,
                self.gc_lifetime,
            ));
        }
    }
}
