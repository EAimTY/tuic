use crate::{
    certificate::{self, CertificateError},
    socks5, Config,
};
use quinn::{ClientConfig as QuinnClientConfig, Endpoint, NewConnection};
use rustls::RootCertStore;
use std::{
    io,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};
use thiserror::Error;
use tokio::net::{self, TcpListener, TcpStream};

pub async fn start(_config: Config) -> Result<(), Error> {
    let quinn_client_config = load_client_config()?;

    let endpoint = {
        let mut endpoint = Endpoint::client(([0, 0, 0, 0], 0).into())?;
        endpoint.set_default_client_config(quinn_client_config);
        endpoint
    };

    let conn = Arc::new(get_connection(&endpoint, "localhost", 5000).await?);

    let listener = TcpListener::bind("0.0.0.0:8888").await.unwrap();

    while let Ok((mut stream, addr)) = listener.accept().await {
        let conn = Arc::clone(&conn);
        /*
                tokio::spawn(async move {
                    // Handshake
                    let hs_req = socks5::HandshakeRequest::read_from(&mut stream)
                        .await
                        .unwrap();
                    if hs_req.methods.contains(&socks5::SOCKS5_AUTH_METHOD_NONE) {
                        let hs_res = socks5::HandshakeResponse::new(socks5::SOCKS5_AUTH_METHOD_NONE);
                        hs_res.write_to(&mut stream).await.unwrap();
                    } else {
                        return;
                    }

                    // Request
                    let tcp_req = socks5::TcpRequestHeader::read_from(&mut stream)
                        .await
                        .unwrap();

                    let (mut send, recv) = conn.connection.open_bi().await.unwrap();
                    tcp_req.write_to(&mut send).await.unwrap();
                    send.finish().await.unwrap();

                    let target_addr = tcp_req.address.to_socket_addrs().unwrap().next().unwrap();
                    let target_stream = TcpStream::connect(&target_addr).await.unwrap();
                    let local_addr = target_stream.local_addr().unwrap();
                });
        */
    }

    Ok(())
}

async fn get_connection(
    endpoint: &Endpoint,
    server_addr: &str,
    server_port: u16,
) -> Result<NewConnection, Error> {
    let socket_addr = net::lookup_host((server_addr, server_port))
        .await
        .unwrap()
        .next()
        .unwrap();

    let conn = endpoint
        .connect(socket_addr, server_addr)
        .unwrap()
        .await
        .unwrap();

    Ok(conn)
}

fn load_client_config() -> Result<QuinnClientConfig, Error> {
    let cert = certificate::load_cert()?;

    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add(&cert).unwrap();

    let client_config = QuinnClientConfig::with_root_certificates(root_cert_store);

    Ok(client_config)
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error("Failed to create the client endpoint")]
    Endpoint(#[from] io::Error),
}
