use self::{connection::Connection, protocol::Error as ProtocolError};
use crate::relay::Request as RelayRequest;
use std::{io::Error as IoError, net::SocketAddr, sync::Arc};
use thiserror::Error;
use tokio::{net::TcpListener, sync::mpsc::Sender as MpscSender};

pub use self::authentication::Authentication;

mod authentication;
mod connection;
mod convert;
mod protocol;

pub struct Socks5 {
    listener: TcpListener,
    authentication: Arc<Authentication>,
    max_udp_packet_size: usize,
    req_tx: MpscSender<RelayRequest>,
}

impl Socks5 {
    pub async fn init(
        local_addr: SocketAddr,
        auth: Authentication,
        max_udp_pkt_size: usize,
        req_tx: MpscSender<RelayRequest>,
    ) -> Result<Self, Socks5Error> {
        let listener = TcpListener::bind(local_addr).await?;
        let auth = Arc::new(auth);

        Ok(Self {
            listener,
            authentication: auth,
            max_udp_packet_size: max_udp_pkt_size,
            req_tx,
        })
    }

    pub async fn run(self) {
        while let Ok((conn, _)) = self.listener.accept().await {
            let auth = self.authentication.clone();
            let req_tx = self.req_tx.clone();

            tokio::spawn(async move {
                match Connection::handle(conn, auth, self.max_udp_packet_size, req_tx).await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
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
    #[error("called associate from a domain address")]
    AssociateFromDomainAddress,
    #[error("fragmented UDP packet is not supported")]
    FragmentedUdpPacket,
    #[error("authentication failed")]
    Authentication,
}
