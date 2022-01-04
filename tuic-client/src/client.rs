use crate::{certificate::CertificateError, Config, ConnectionManager, Socks5Server};
use std::io;
use thiserror::Error;

pub async fn start(config: Config) -> Result<(), Error> {
    let (conn_mgr, channel_msg_sender) = ConnectionManager::new(&config)?;
    let socks5_server = Socks5Server::new(&config, channel_msg_sender);

    conn_mgr.run().await;
    socks5_server.run().await?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to create the client endpoint")]
    Endpoint(#[from] io::Error),
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error("TUIC Authentication failed")]
    AuthFailed,
    #[error("Socks5 Authentication failed")]
    Socks5AuthFailed,
}
