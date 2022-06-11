#![allow(unused)]

use self::{
    connection::ConnectionConfig,
    incoming::{NextDatagrams, NextIncomingUniStreams},
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

pub use self::{
    address::Address, connection::Connection, register::Register, request::Request,
    stream::BiStream,
};

mod address;
mod connection;
mod incoming;
mod register;
mod request;
mod stream;

pub async fn init(
    quinn_config: ClientConfig,
    server_addr: ServerAddr,
    token_digest: [u8; 32],
    heartbeat_interval: u64,
    reduce_rtt: bool,
    udp_relay_mode: UdpRelayMode,
) -> (JoinHandle<()>, Sender<Request>) {
    let (req_tx, req_rx) = mpsc::channel(1);

    let config = ConnectionConfig::new(
        quinn_config,
        server_addr,
        token_digest,
        heartbeat_interval,
        reduce_rtt,
    );

    let conn_uninit = unsafe { Arc::new(AsyncMutex::new(MaybeUninit::uninit().assume_init())) }; // TODO: fix UB
    let conn_lock = conn_uninit.clone().lock_owned().await;
    let conn = conn_uninit.clone();

    let (dg_next_rx, dg_next_tx, dg_is_closed) = NextDatagrams::new();
    let (uni_next_rx, uni_next_tx, uni_is_closed) = NextIncomingUniStreams::new();

    let guard_connection = connection::guard_connection(
        config,
        conn_uninit,
        conn_lock,
        udp_relay_mode,
        dg_next_tx,
        uni_next_tx,
        dg_is_closed,
        uni_is_closed,
    );
    let listen_request = request::listen_request(conn, req_rx);
    let listen_incoming = incoming::listen_incoming(udp_relay_mode, dg_next_rx, uni_next_rx);

    let task = tokio::spawn(async move {
        tokio::select! {
            () = guard_connection => (),
            () = listen_request => (),
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
pub enum UdpRelayMode {
    Native,
    Quic,
}
