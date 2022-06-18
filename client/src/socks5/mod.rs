use crate::relay::Request as RelayRequest;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use socks5_server::{Auth, Connection, IncomingConnection, Server};
use std::{
    future::Future,
    io::Result,
    net::{SocketAddr, TcpListener as StdTcpListener},
    sync::Arc,
};
use tokio::{net::TcpListener, sync::mpsc::Sender};

mod associate;
mod bind;
mod connect;

pub async fn init(
    local_addr: SocketAddr,
    auth: Arc<dyn Auth + Send + Sync>,
    req_tx: Sender<RelayRequest>,
) -> Result<impl Future<Output = ()>> {
    let socks5 = Socks5::init(local_addr, auth, req_tx).await?;
    Ok(socks5.run())
}

struct Socks5 {
    server: Server,
    req_tx: Sender<RelayRequest>,
}

impl Socks5 {
    async fn init(
        local_addr: SocketAddr,
        auth: Arc<dyn Auth + Send + Sync>,
        req_tx: Sender<RelayRequest>,
    ) -> Result<Self> {
        let listener = if local_addr.is_ipv4() {
            TcpListener::bind(local_addr).await?
        } else {
            let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
            socket.set_only_v6(false)?;
            socket.bind(&SockAddr::from(local_addr))?;
            socket.listen(128)?;
            TcpListener::from_std(StdTcpListener::from(socket))?
        };

        let server = Server::new(listener, auth);

        Ok(Self { server, req_tx })
    }

    async fn run(self) {
        async fn handle_connection(
            conn: IncomingConnection,
            req_tx: Sender<RelayRequest>,
        ) -> Result<()> {
            match conn.handshake().await? {
                Connection::Connect(conn, addr) => connect::handle(conn, req_tx, addr).await,
                Connection::Bind(conn, addr) => bind::handle(conn, req_tx, addr).await,
                Connection::Associate(conn, addr) => associate::handle(conn, req_tx, addr).await,
            }
        }

        match self.server.local_addr() {
            Ok(addr) => log::info!("[socks5] Started. Listening: {addr}"),
            Err(err) => {
                log::error!("[socks5] Failed to get local socks5 server address: {err}");
                return;
            }
        }

        loop {
            let (conn, addr) = match self.server.accept().await {
                Ok((conn, addr)) => {
                    log::debug!("[socks5] [{addr}] [establish]");
                    (conn, addr)
                }
                Err(err) => {
                    log::warn!("[socks5] Failed to accept connection: {err}");
                    continue;
                }
            };

            let req_tx = self.req_tx.clone();

            tokio::spawn(async move {
                match handle_connection(conn, req_tx).await {
                    Ok(()) => log::debug!("[socks5] [{addr}] [disconnect]"),
                    Err(err) => log::warn!("[socks5] [{addr}] {err}"),
                }
            });
        }
    }
}
