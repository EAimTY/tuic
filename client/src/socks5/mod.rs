use crate::relay::Request as RelayRequest;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use socks5_server::{Auth, Connection, IncomingConnection, Server};
use std::{
    io::Error as IoError,
    net::{SocketAddr, TcpListener as StdTcpListener},
    sync::Arc,
};
use tokio::{net::TcpListener, sync::mpsc::Sender};

mod associate;
mod bind;
mod connect;

pub struct Socks5 {
    server: Server,
    local_addr: SocketAddr,
    req_tx: Sender<RelayRequest>,
}

impl Socks5 {
    pub async fn init(
        local_addr: SocketAddr,
        auth: Arc<dyn Auth + Send + Sync + 'static>,
        req_tx: Sender<RelayRequest>,
    ) -> Result<Self, IoError> {
        let listener = if local_addr.is_ipv4() {
            TcpListener::bind(local_addr).await?
        } else {
            let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
            socket.set_only_v6(false)?;
            socket.bind(&SockAddr::from(local_addr))?;
            socket.listen(128)?;
            TcpListener::from_std(StdTcpListener::from(socket))?
        };

        let local_addr = listener.local_addr()?;
        let server = Server::new(listener, auth);

        Ok(Self {
            server,
            local_addr,
            req_tx,
        })
    }

    pub async fn run(self) {
        async fn handle_connection(
            conn: IncomingConnection,
            req_tx: Sender<RelayRequest>,
        ) -> Result<(), IoError> {
            match conn.handshake().await? {
                Connection::Connect(conn, addr) => connect::handle(conn, req_tx, addr).await,
                Connection::Bind(conn, addr) => bind::handle(conn, req_tx, addr).await,
                Connection::Associate(conn, addr) => associate::handle(conn, req_tx, addr).await,
            }
        }

        log::info!("[socks5] started. Listening: {}", self.local_addr);

        while let Ok((conn, src_addr)) = self.server.accept().await {
            let req_tx = self.req_tx.clone();

            tokio::spawn(async move {
                match handle_connection(conn, req_tx).await {
                    Ok(()) => {}
                    Err(err) => log::warn!("[socks5] [{src_addr}] {err}"),
                }
            });
        }
    }
}
