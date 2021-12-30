use crate::{
    certificate::{self, CertificateError},
    Config,
};
use quinn::{ClientConfig as QuinnClientConfig, Endpoint};
use rustls::RootCertStore;
use std::{io, net::SocketAddr};
use thiserror::Error;

pub async fn start(_config: Config) -> Result<(), ClientError> {
    let client_config = load_client_config()?;

    let mut client = Endpoint::client(([0, 0, 0, 0], 0).into())?;
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

fn load_client_config() -> Result<QuinnClientConfig, ClientConfigError> {
    let cert = certificate::load_cert()?;

    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add(&cert).unwrap();

    let client_config = QuinnClientConfig::with_root_certificates(root_cert_store);

    Ok(client_config)
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    ClientConfig(#[from] ClientConfigError),
    #[error("Failed to create the client endpoint")]
    Endpoint(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum ClientConfigError {
    #[error(transparent)]
    Certificate(#[from] CertificateError),
}
