use crate::{
    certificate::{self, CertificateError, PrivateKeyError},
    config::ServerConfig,
};
use futures_util::StreamExt;
use quinn::{ConnectionError, Endpoint, Incoming, ServerConfig as QuinnServerConfig};
use std::io;
use thiserror::Error;

pub async fn start(_config: ServerConfig) -> Result<(), ServerError> {
    let server_config = load_server_config()?;

    let (_, incoming) = Endpoint::server(server_config, ([127, 0, 0, 1], 5000).into())?;
    handle_server(incoming).await;

    Ok(())
}

async fn handle_server(mut incoming: Incoming) {
    while let Some(conn) = incoming.next().await {
        let mut conn = conn.await.unwrap();

        tokio::spawn(async move {
            while let Some(stream) = conn.bi_streams.next().await {
                match stream {
                    Ok((_send, recv)) => {
                        println!("Server received a msg!");
                        let msg = recv.read_to_end(10).await.unwrap();
                        println!("Server received: {:?}", std::str::from_utf8(&msg).unwrap());
                    }
                    Err(ConnectionError::ApplicationClosed { .. }) => {
                        println!("Connection closed");
                        break;
                    }
                    Err(e) => {
                        println!("Connection error: {:?}", e);
                        break;
                    }
                }
            }
        });
    }
}

fn load_server_config() -> Result<QuinnServerConfig, ServerConfigError> {
    let cert = certificate::load_cert()?;
    let priv_key = certificate::load_priv_key()?;

    let server_config = QuinnServerConfig::with_single_cert(vec![cert], priv_key)?;

    Ok(server_config)
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error(transparent)]
    ServerConfig(#[from] ServerConfigError),
    #[error("Failed to create the server endpoint")]
    Endpoint(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum ServerConfigError {
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error(transparent)]
    PrivateKey(#[from] PrivateKeyError),
    #[error(transparent)]
    Rustls(#[from] rustls::Error),
}
