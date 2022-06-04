#![allow(unused)]

use self::incoming::{NextDatagrams, NextIncomingUniStreams};
use quinn::{ClientConfig, Endpoint, EndpointConfig};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    io::Error as IoError,
    mem::MaybeUninit,
    net::{Ipv6Addr, SocketAddr, UdpSocket},
    sync::Arc,
};
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex as AsyncMutex,
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

pub struct Relay {
    req_rx: Receiver<Request>,
    endpoint: Endpoint,
    server_addr: ServerAddr,
    token_digest: [u8; 32],
    udp_relay_mode: UdpRelayMode,
    heartbeat_interval: u64,
    reduce_rtt: bool,
}

impl Relay {
    pub fn init(
        config: ClientConfig,
        server_addr: ServerAddr,
        token_digest: [u8; 32],
        udp_relay_mode: UdpRelayMode,
        heartbeat_interval: u64,
        reduce_rtt: bool,
    ) -> Result<(Self, Sender<Request>), IoError> {
        let mut endpoint = {
            let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;

            socket.set_only_v6(false)?;
            socket.bind(&SockAddr::from(SocketAddr::from((
                Ipv6Addr::UNSPECIFIED,
                0,
            ))))?;

            Endpoint::new(EndpointConfig::default(), None, UdpSocket::from(socket))?.0
        };

        endpoint.set_default_client_config(config);

        let (req_tx, req_rx) = mpsc::channel(1);

        let relay = Self {
            req_rx,
            endpoint,
            server_addr,
            token_digest,
            udp_relay_mode,
            heartbeat_interval,
            reduce_rtt,
        };

        Ok((relay, req_tx))
    }

    pub async fn run(mut self) {
        let conn_uninit = unsafe { Arc::new(AsyncMutex::new(MaybeUninit::uninit().assume_init())) };
        let conn_lock = conn_uninit.clone().lock_owned().await;
        let conn = conn_uninit.clone();

        let (dg_next_rx, dg_next_tx) = NextDatagrams::new();
        let (uni_next_rx, uni_next_tx) = NextIncomingUniStreams::new();

        tokio::select! {
            () = connection::guard_connection(conn_uninit, conn_lock, self.udp_relay_mode, dg_next_tx, uni_next_tx) => (),
            () = request::listen_request(conn, self.req_rx) => (),
            () = incoming::listen_incoming(self.udp_relay_mode, dg_next_rx, uni_next_rx) => (),
        };
    }
}

pub enum ServerAddr {
    SocketAddr {
        server_addr: SocketAddr,
        server_name: String,
    },
    HostnameAddr {
        hostname: String,
        server_port: u16,
    },
}

impl Display for ServerAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            ServerAddr::SocketAddr {
                server_addr,
                server_name,
            } => write!(f, "{server_addr} ({server_name})"),
            ServerAddr::HostnameAddr {
                hostname,
                server_port,
            } => write!(f, "{hostname}:{server_port}"),
        }
    }
}

#[derive(Clone, Copy)]
pub enum UdpRelayMode {
    Native,
    Quic,
}
