use crate::{certificate, config::ServerConfig};
use anyhow::Result;
use futures_util::StreamExt;
use quinn::{Endpoint, Incoming, ServerConfig as QuinnServerConfig};

pub async fn start(_config: ServerConfig) -> Result<()> {
    let server_config = load_server_config()?;

    let (_, incoming) = Endpoint::server(server_config, ([127, 0, 0, 1], 5000).into())?;
    handle_server(incoming).await;

    Ok(())
}

async fn handle_server(mut incoming: Incoming) {
    while let Some(conn) = incoming.next().await {
        let mut conn = conn.await.unwrap();

        while let Some(Ok((_, recv))) = conn.bi_streams.next().await {
            println!("Server received a msg!");
            let msg = recv.read_to_end(10).await.unwrap();
            println!("Server received: {:?}", std::str::from_utf8(&msg).unwrap());
        }
    }
}

fn load_server_config() -> Result<QuinnServerConfig> {
    let cert = certificate::load_cert()?;
    let priv_key = certificate::load_priv_key()?;

    let server_config = QuinnServerConfig::with_single_cert(vec![cert], priv_key)?;

    Ok(server_config)
}
