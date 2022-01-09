use crate::{certificate, Config, Connection, ServerError};
use futures_util::StreamExt;
use quinn::{Endpoint, ServerConfig as QuinnServerConfig};
use std::net::SocketAddr;

pub async fn start(config: Config) -> Result<(), ServerError> {
    let server_config = load_server_config(&config.certificate_path, &config.private_key_path)?;

    let (_, mut incoming) =
        Endpoint::server(server_config, SocketAddr::from(([0, 0, 0, 0], config.port)))?;

    while let Some(conn) = incoming.next().await {
        tokio::spawn(async move {
            let conn = match Connection::new(conn).await {
                Ok(conn) => conn,
                Err(_err) => {
                    return;
                }
            };

            conn.process(config.token).await;
        });
    }

    Ok(())
}

fn load_server_config(cert_path: &str, key_path: &str) -> Result<QuinnServerConfig, ServerError> {
    let cert = certificate::load_cert(cert_path)?;
    let priv_key = certificate::load_priv_key(key_path)?;

    let server_config = QuinnServerConfig::with_single_cert(vec![cert], priv_key)?;

    Ok(server_config)
}
