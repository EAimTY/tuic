use crate::{certificate, config::ClientConfig};
use anyhow::Result;
use quinn::{ClientConfig as QuinnClientConfig, Endpoint};
use rustls::RootCertStore;
use std::net::SocketAddr;

pub async fn start(_config: ClientConfig) -> Result<()> {
    let client_config = load_client_config()?;

    let mut client = Endpoint::client(([127, 0, 0, 1], 5001).into())?;
    client.set_default_client_config(client_config);

    handle_client(client, ([127, 0, 0, 1], 5000).into(), "localhost").await;

    Ok(())
}

async fn handle_client(client: Endpoint, server_addr: SocketAddr, server_name: &str) {
    let conn = client
        .connect(server_addr, server_name)
        .unwrap()
        .await
        .unwrap();

    let (mut send, _) = conn.connection.open_bi().await.unwrap();

    println!("Sending msg to server...");
    send.write_all(b"a msg").await.unwrap();
    send.finish().await.unwrap();
    println!("msg was sent!");
}

fn load_client_config() -> Result<QuinnClientConfig> {
    let cert = certificate::load_cert()?;

    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add(&cert)?;

    let client_config = QuinnClientConfig::with_root_certificates(root_cert_store);

    Ok(client_config)
}
