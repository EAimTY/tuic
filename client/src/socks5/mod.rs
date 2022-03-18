use self::connection::Connection;
use crate::relay::Request as RelayRequest;
use anyhow::Result;
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::mpsc::Sender as MpscSender};

pub use self::authentication::Authentication;

mod authentication;
mod connection;
mod convert;
mod protocol;

pub struct Socks5 {
    listener: TcpListener,
    authentication: Arc<Authentication>,
    req_tx: MpscSender<RelayRequest>,
}

impl Socks5 {
    pub async fn init(
        local_addr: SocketAddr,
        auth: Authentication,
        req_tx: MpscSender<RelayRequest>,
    ) -> Result<Self> {
        let listener = TcpListener::bind(local_addr).await?;
        let auth = Arc::new(auth);

        Ok(Self {
            listener,
            authentication: auth,
            req_tx,
        })
    }

    pub async fn run(self) {
        while let Ok((conn, _)) = self.listener.accept().await {
            let auth = self.authentication.clone();
            let req_tx = self.req_tx.clone();

            tokio::spawn(async move {
                match Connection::handle(conn, auth, req_tx).await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                }
            });
        }
    }
}
