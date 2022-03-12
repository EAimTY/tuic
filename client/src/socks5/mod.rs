use self::connection::Connection;
use crate::relay::Request as RelayRequest;
use anyhow::Result;
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::mpsc::Sender as MpscSender, task::JoinHandle};

pub use self::auth::Authentication;

mod auth;
mod connection;
mod protocol;

pub async fn init(
    local_addr: SocketAddr,
    auth: Authentication,
    req_tx: MpscSender<RelayRequest>,
) -> Result<JoinHandle<()>> {
    let listener = TcpListener::bind(local_addr).await?;
    let auth = Arc::new(auth);

    Ok(tokio::spawn(listen_socks5_request(listener, auth, req_tx)))
}

async fn listen_socks5_request(
    listener: TcpListener,
    auth: Arc<Authentication>,
    req_tx: MpscSender<RelayRequest>,
) {
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(Connection::handle_stream(
            stream,
            auth.clone(),
            req_tx.clone(),
        ));
    }
}
