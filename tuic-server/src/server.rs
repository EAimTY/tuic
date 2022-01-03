use crate::{
    certificate::{self, CertificateError, PrivateKeyError},
    Config,
};
use futures_util::StreamExt;
use quinn::{ConnectionError, Endpoint, ServerConfig as QuinnServerConfig};
use std::{io, net::ToSocketAddrs};
use thiserror::Error;
use tokio::net::TcpStream;

pub async fn start(_config: Config) -> Result<(), ServerError> {
    let server_config = load_server_config()?;

    let (_, mut incoming) = Endpoint::server(server_config, ([127, 0, 0, 1], 5000).into())?;

    while let Some(conn) = incoming.next().await {
        let mut conn = conn.await.unwrap();

        tokio::spawn(async move {
            // Handsake
            let (mut auth_send, mut auth_recv) = conn.bi_streams.next().await.unwrap().unwrap();
            let _hs_req = tuic_protocol::HandshakeRequest::read_from(&mut auth_recv)
                .await
                .unwrap();

            let hs_res = tuic_protocol::HandshakeResponse::new(true);
            hs_res.write_to(&mut auth_send).await.unwrap();
            auth_send.finish().await.unwrap();

            while let Some(stream) = conn.bi_streams.next().await {
                match stream {
                    Ok((mut send, mut recv)) => {
                        tokio::spawn(async move {
                            let connect_req = tuic_protocol::ConnectRequest::read_from(&mut recv)
                                .await
                                .unwrap();

                            let addr = connect_req
                                .address
                                .to_socket_addrs()
                                .unwrap()
                                .next()
                                .unwrap();
                            let mut stream = TcpStream::connect(addr).await.unwrap();

                            let connect_res = tuic_protocol::ConnectResponse::new(
                                tuic_protocol::Reply::Succeeded,
                            );
                            connect_res.write_to(&mut send).await.unwrap();

                            let (mut local_recv, mut local_send) = stream.split();

                            let remote_to_local = tokio::io::copy(&mut recv, &mut local_send);
                            let local_to_remote = tokio::io::copy(&mut local_recv, &mut send);
                            let _ = tokio::try_join!(remote_to_local, local_to_remote);
                        });
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

    Ok(())
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
