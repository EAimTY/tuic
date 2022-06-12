#![allow(unused)]

use self::{
    connection::ConnectionConfig,
    incoming::Receiver as IncomingReceiver,
    stream::{IncomingDatagrams, IncomingUniStreams},
};
use quinn::{ClientConfig, Endpoint, EndpointConfig};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    io::Error as IoError,
    mem::MaybeUninit,
    net::{Ipv6Addr, SocketAddr, UdpSocket},
    sync::Arc,
};
use tokio::{
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex as AsyncMutex,
    },
    task::JoinHandle,
};

pub use self::{address::Address, connection::Connection, request::Request};

mod address;
mod connection;
mod incoming;
mod request;
mod stream;
mod task;

pub async fn init(
    quinn_config: ClientConfig,
    server_addr: ServerAddr,
    token_digest: [u8; 32],
    heartbeat_interval: u64,
    reduce_rtt: bool,
    udp_relay_mode: UdpRelayMode<(), ()>,
) -> (JoinHandle<()>, Sender<Request>) {
    let (req_tx, req_rx) = mpsc::channel(1);

    let config = ConnectionConfig::new(
        quinn_config,
        server_addr,
        token_digest,
        udp_relay_mode,
        heartbeat_interval,
        reduce_rtt,
    );

    let conn = unsafe { Arc::new(AsyncMutex::new(MaybeUninit::uninit().assume_init())) }; // TODO: fix UB
    let conn_lock = conn.clone().lock_owned().await;

    let (incoming_tx, incoming_rx) = match udp_relay_mode {
        UdpRelayMode::Native(()) => {
            let (tx, rx) = incoming::channel::<IncomingDatagrams>();
            (UdpRelayMode::Native(tx), UdpRelayMode::Native(rx))
        }
        UdpRelayMode::Quic(()) => {
            let (tx, rx) = incoming::channel::<IncomingUniStreams>();
            (UdpRelayMode::Quic(tx), UdpRelayMode::Quic(rx))
        }
    };

    let (listen_requests, wait_req) = request::listen_requests(conn.clone(), req_rx);
    let listen_incoming = incoming::listen_incoming(incoming_rx);

    let manage_connection =
        connection::manage_connection(config, conn, conn_lock, incoming_tx, wait_req);

    let task = tokio::spawn(async move {
        tokio::select! {
            () = manage_connection => (),
            () = listen_requests => (),
            () = listen_incoming => (),
        }
    });

    (task, req_tx)
}

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
