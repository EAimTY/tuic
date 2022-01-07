use crate::{certificate, Config, Connection, ServerError};
use futures_util::StreamExt;
use quinn::{Endpoint, ServerConfig as QuinnServerConfig};

pub async fn start(_config: Config) -> Result<(), ServerError> {
    let server_config = load_server_config()?;

    let (_, mut incoming) = Endpoint::server(server_config, ([127, 0, 0, 1], 5000).into())?;

    while let Some(conn) = incoming.next().await {
        tokio::spawn(async move {
            let conn = match Connection::new(conn).await {
                Ok(conn) => conn,
                Err(_err) => {
                    return;
                }
            };

            conn.process().await;
        });
    }

    Ok(())
}

fn load_server_config() -> Result<QuinnServerConfig, ServerError> {
    let cert = certificate::load_cert()?;
    let priv_key = certificate::load_priv_key()?;

    let server_config = QuinnServerConfig::with_single_cert(vec![cert], priv_key)?;

    Ok(server_config)
}
