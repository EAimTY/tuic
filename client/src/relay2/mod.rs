#![allow(unused)]

use quinn::Endpoint;
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::mpsc::Receiver;

pub use self::{address::Address, register::Register, request::Request, stream::BiStream};

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
