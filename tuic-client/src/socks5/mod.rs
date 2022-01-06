use crate::{
    connection::{ConnectionRequest, ConnectionResponse},
    ClientError, Config,
};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::AsyncWriteExt,
    net::{tcp, TcpListener, TcpStream},
    sync::mpsc,
};

mod convert;
mod protocol;

pub struct Socks5Server {
    request_sender: Arc<mpsc::Sender<ConnectionRequest>>,
}

impl Socks5Server {
    pub fn new(_config: &Config, sender: mpsc::Sender<ConnectionRequest>) -> Self {
        Self {
            request_sender: Arc::new(sender),
        }
    }

    pub async fn run(&self) -> Result<(), ClientError> {
        let socks5_listener = TcpListener::bind("0.0.0.0:8887").await.unwrap();

        while let Ok((stream, _)) = socks5_listener.accept().await {
            let socks5_conn = Socks5Connection::new(stream, &self.request_sender);
            socks5_conn.process().await;
        }

        Ok(())
    }
}

struct Socks5Connection {
    stream: TcpStream,
    request_sender: Arc<mpsc::Sender<ConnectionRequest>>,
}

impl Socks5Connection {
    fn new(stream: TcpStream, request_sender: &Arc<mpsc::Sender<ConnectionRequest>>) -> Self {
        Self {
            stream,
            request_sender: Arc::clone(request_sender),
        }
    }

    async fn process(mut self) {
        tokio::spawn(async move {
            self.handshake().await.unwrap();

            let socks5_req = protocol::ConnectRequest::read_from(&mut self.stream)
                .await
                .unwrap();

            let (req, res_receiver) =
                ConnectionRequest::new(socks5_req.command.into(), socks5_req.address.into());

            self.request_sender.send(req).await.map_err(|_| ()).unwrap();

            match res_receiver.await.unwrap() {
                Ok((mut remote_send, mut remote_recv)) => {
                    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();

                    let addr = listener.local_addr().unwrap();
                    let tcp_res =
                        protocol::ConnectResponse::new(protocol::Reply::Succeeded, addr.into());
                    tcp_res.write_to(&mut self.stream).await.unwrap();
                    self.stream.shutdown().await.unwrap();

                    let (stream, _) = listener.accept().await.unwrap();
                    let (mut local_recv, mut local_send) = stream.into_split();

                    self.forward(
                        &mut remote_send,
                        &mut remote_recv,
                        &mut local_send,
                        &mut local_recv,
                    )
                    .await;
                }
                Err(err) => {
                    let tcp_res = protocol::ConnectResponse::new(
                        protocol::Reply::GeneralFailure,
                        SocketAddr::from(([0, 0, 0, 0], 0)).into(),
                    );
                    tcp_res.write_to(&mut self.stream).await.unwrap();
                    self.stream.shutdown().await.unwrap();
                }
            }
        });
    }

    async fn forward(
        &self,
        remote_send: &mut quinn::SendStream,
        remote_recv: &mut quinn::RecvStream,
        local_send: &mut tcp::OwnedWriteHalf,
        local_recv: &mut tcp::OwnedReadHalf,
    ) {
        let remote_to_local = tokio::io::copy(remote_recv, local_send);
        let local_to_remote = tokio::io::copy(local_recv, remote_send);
        let _ = tokio::try_join!(remote_to_local, local_to_remote);
    }

    async fn handshake(&mut self) -> Result<(), ClientError> {
        let hs_req = protocol::HandshakeRequest::read_from(&mut self.stream)
            .await
            .unwrap();
        if hs_req
            .methods
            .contains(&protocol::handshake::SOCKS5_AUTH_METHOD_NONE)
        {
            let hs_res =
                protocol::HandshakeResponse::new(protocol::handshake::SOCKS5_AUTH_METHOD_NONE);
            hs_res.write_to(&mut self.stream).await.unwrap();
        } else {
            return Err(ClientError::Socks5AuthFailed);
        }

        Ok(())
    }
}
