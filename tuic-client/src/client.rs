use crate::{certificate::CertificateError, Config, ConnectionManager, Socks5Server};
use std::{error::Error, io};
use thiserror::Error;

pub async fn start(config: Config) -> Result<(), ClientError> {
    let (conn_mgr, channel_msg_sender) = ConnectionManager::new(&config)?;
    conn_mgr.run().await;

    let socks5_server = Socks5Server::new(&config, channel_msg_sender);
    socks5_server.run().await?;

    Ok(())
}

pub fn exit(err: Box<dyn Error>) -> ! {
    eprintln!("{}", err);
    std::process::exit(1);
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error("Failed to create the client endpoint")]
    Endpoint(#[from] io::Error),
    #[error("Failed to get connection from the pool")]
    GetConnection,
    #[error("TUIC Authentication failed")]
    AuthFailed,
    #[error("Socks5 Authentication failed")]
    Socks5AuthFailed,
}
