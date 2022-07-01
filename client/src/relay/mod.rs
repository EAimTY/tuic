use self::{connection::ConnectionConfig, stream::IncomingUniStreams};
use quinn::{ClientConfig, Datagrams};
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    future::Future,
    net::SocketAddr,
    sync::{atomic::AtomicUsize, Arc},
};
use tokio::sync::{
    mpsc::{self, Sender},
    Mutex as AsyncMutex,
};

pub use self::{address::Address, connection::Connection, request::Request};

mod address;
mod connection;
mod incoming;
mod request;
mod stream;
mod task;

pub static MAX_UDP_RELAY_PACKET_SIZE: AtomicUsize = AtomicUsize::new(1500);

#[allow(clippy::too_many_arguments)]
pub async fn init(
    quinn_config: ClientConfig,
    server_addr: ServerAddr,
    token_digest: [u8; 32],
    heartbeat_interval: u64,
    reduce_rtt: bool,
    udp_relay_mode: UdpRelayMode<(), ()>,
    req_timeout: u64,
    max_udp_relay_packet_size: usize,
) -> (impl Future<Output = ()>, Sender<Request>) {
    let (req_tx, req_rx) = mpsc::channel(1);

    let config = ConnectionConfig::new(
        quinn_config,
        server_addr.clone(),
        token_digest,
        udp_relay_mode,
        heartbeat_interval,
        reduce_rtt,
        max_udp_relay_packet_size,
    );

    let conn = Arc::new(AsyncMutex::new(None));
    let conn_lock = conn.clone().lock_owned().await;

    let (incoming_tx, incoming_rx) = match udp_relay_mode {
        UdpRelayMode::Native(()) => {
            let (tx, rx) = incoming::channel::<Datagrams>();
            (UdpRelayMode::Native(tx), UdpRelayMode::Native(rx))
        }
        UdpRelayMode::Quic(()) => {
            let (tx, rx) = incoming::channel::<IncomingUniStreams>();
            (UdpRelayMode::Quic(tx), UdpRelayMode::Quic(rx))
        }
    };

    let (listen_requests, wait_req) = request::listen_requests(conn.clone(), req_rx, req_timeout);
    let listen_incoming = incoming::listen_incoming(incoming_rx);

    let manage_connection =
        connection::manage_connection(config, conn, conn_lock, incoming_tx, wait_req);

    let task = async move {
        log::info!("[relay] Started. Target server: {server_addr}");

        tokio::select! {
            _ = tokio::spawn(manage_connection) => {}
            _ = tokio::spawn(listen_requests) => {}
            _ = tokio::spawn(listen_incoming) => {}
        }
    };

    (task, req_tx)
}

#[derive(Clone)]
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

#[derive(Clone, Copy)]
pub enum UdpRelayMode<N, Q> {
    Native(N),
    Quic(Q),
}
