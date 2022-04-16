use self::{connection::Connection, protocol::Error as ProtocolError};
use crate::relay::Request as RelayRequest;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    io::Error as IoError,
    net::{SocketAddr, TcpListener as StdTcpListener},
    sync::Arc,
};
use thiserror::Error;
use tokio::{net::TcpListener, sync::mpsc::Sender};

pub use self::authentication::Authentication;

mod authentication;
mod connection;
mod convert;
mod protocol;

pub struct Socks5 {
    listener: TcpListener,
    local_addr: SocketAddr,
    authentication: Arc<Authentication>,
    max_udp_packet_size: usize,
    req_tx: Sender<RelayRequest>,
}

impl Socks5 {
    pub async fn init(
        local_addr: SocketAddr,
        auth: Authentication,
        max_udp_pkt_size: usize,
        req_tx: Sender<RelayRequest>,
    ) -> Result<Self, Socks5Error> {
        let listener = if local_addr.is_ipv4() {
            TcpListener::bind(local_addr).await?
        } else {
            let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
            socket.set_only_v6(false)?;
            socket.bind(&SockAddr::from(local_addr))?;
            socket.listen(128)?;
            TcpListener::from_std(StdTcpListener::from(socket))?
        };

        let auth = Arc::new(auth);

        Ok(Self {
            listener,
            local_addr,
            authentication: auth,
            max_udp_packet_size: max_udp_pkt_size,
            req_tx,
        })
    }

    pub async fn run(self) {
        log::info!("[socks5] started. Listening: {}", self.local_addr);

        while let Ok((conn, src_addr)) = self.listener.accept().await {
            let auth = self.authentication.clone();
            let req_tx = self.req_tx.clone();

            tokio::spawn(async move {
                match Connection::handle(
                    conn,
                    src_addr,
                    self.local_addr,
                    auth,
                    self.max_udp_packet_size,
                    req_tx,
                )
                .await
                {
                    Ok(()) => {}
                    Err(err) => log::warn!("[socks5] [{src_addr}] {err}"),
                }
            });
        }
    }
}

#[derive(Debug, Error)]
pub enum Socks5Error {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Io(#[from] IoError),
    #[error("failed to connect to the relay layer")]
    RelayConnectivity,
    #[error("fragmented UDP packet is not supported")]
    FragmentedUdpPacket,
    #[error("authentication failed")]
    Authentication,
}
