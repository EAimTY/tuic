use crate::{
    certificate::{self, CertificateError},
    socks5, Config,
};
use quinn::{ClientConfig as QuinnClientConfig, Endpoint, NewConnection};
use rustls::RootCertStore;
use std::{io, sync::Arc};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt,
    net::{self, TcpListener},
};

pub async fn start(_config: Config) -> Result<(), Error> {
    let quinn_client_config = load_client_config()?;

    let endpoint = {
        let mut endpoint = Endpoint::client(([0, 0, 0, 0], 0).into())?;
        endpoint.set_default_client_config(quinn_client_config);
        endpoint
    };

    let conn = Arc::new(get_connection(&endpoint, "localhost", 5000).await?);

    let (mut auth_send, mut auth_recv) = conn.connection.open_bi().await.unwrap();

    let tuic_hs_req = tuic_protocol::HandshakeRequest::new(Vec::new());
    tuic_hs_req.write_to(&mut auth_send).await?;
    auth_send.finish().await.unwrap();

    let tuic_hs_res = tuic_protocol::HandshakeResponse::read_from(&mut auth_recv)
        .await
        .unwrap();
    if !tuic_hs_res.is_succeeded {
        return Err(Error::AuthFailed);
    }

    let listener = TcpListener::bind("0.0.0.0:8887").await.unwrap();

    while let Ok((mut stream, _)) = listener.accept().await {
        let conn = Arc::clone(&conn);
        tokio::spawn(async move {
            let hs_req = socks5::HandshakeRequest::read_from(&mut stream)
                .await
                .unwrap();
            if hs_req
                .methods
                .contains(&socks5::handshake::SOCKS5_AUTH_METHOD_NONE)
            {
                let hs_res =
                    socks5::HandshakeResponse::new(socks5::handshake::SOCKS5_AUTH_METHOD_NONE);
                hs_res.write_to(&mut stream).await.unwrap();
            } else {
                return;
            }

            let tcp_req = socks5::ConnectRequest::read_from(&mut stream)
                .await
                .unwrap();

            let (mut send, mut recv) = conn.connection.open_bi().await.unwrap();

            let tuic_connect_req = tuic_protocol::ConnectRequest::from(tcp_req);
            tuic_connect_req.write_to(&mut send).await.unwrap();

            let tuic_connect_res = tuic_protocol::ConnectResponse::read_from(&mut recv)
                .await
                .unwrap();

            if tuic_connect_res.is_succeeded() {
                let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();

                let addr = listener.local_addr().unwrap();
                let tcp_res = socks5::ConnectResponse::new(socks5::Reply::Succeeded, addr.into());
                tcp_res.write_to(&mut stream).await.unwrap();
                stream.shutdown().await.unwrap();

                let (mut stream, _) = listener.accept().await.unwrap();
                let (mut local_recv, mut local_send) = stream.split();

                let remote_to_local = tokio::io::copy(&mut recv, &mut local_send);
                let local_to_remote = tokio::io::copy(&mut local_recv, &mut send);
                let _ = tokio::try_join!(remote_to_local, local_to_remote);
            }
        });
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
    #[error("Failed to create the client endpoint")]
    Endpoint(#[from] io::Error),
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    #[error("TUIC Authentication failed")]
    AuthFailed,
}
