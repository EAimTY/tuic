use self::protocol::{
    handshake, Error as Socks5Error, HandshakeRequest, HandshakeResponse, Reply, Request, Response,
};
use crate::{
    connection::{ConnectionError as TuicConnectionError, ConnectionRequest},
    ClientError, Config,
};
use quinn::{RecvStream as QuinnRecvStream, SendStream as QuinnSendStream};
use std::{io::Error as IoError, net::SocketAddr, sync::Arc};
use thiserror::Error;
use tokio::{
    io,
    net::{TcpListener, TcpStream},
    sync::mpsc::Sender as MpscSender,
};

mod convert;
mod protocol;

pub struct Socks5Server {
    request_sender: Arc<MpscSender<ConnectionRequest>>,
    local_addr: SocketAddr,
}

impl Socks5Server {
    pub fn new(config: &Config, sender: MpscSender<ConnectionRequest>) -> Self {
        Self {
            request_sender: Arc::new(sender),
            local_addr: config.local_addr,
        }
    }

    pub async fn run(self) -> Result<(), ClientError> {
        let socks5_listener = TcpListener::bind(self.local_addr).await?;

        while let Ok((stream, _)) = socks5_listener.accept().await {
            let mut socks5_conn = Socks5Connection::new(stream, &self.request_sender);

            tokio::spawn(async move { if let Err(_err) = socks5_conn.process().await {} });
        }

        Ok(())
    }
}

struct Socks5Connection {
    stream: TcpStream,
    request_sender: Arc<MpscSender<ConnectionRequest>>,
}

impl Socks5Connection {
    fn new(stream: TcpStream, request_sender: &Arc<MpscSender<ConnectionRequest>>) -> Self {
        Self {
            stream,
            request_sender: Arc::clone(request_sender),
        }
    }

    async fn process(&mut self) -> Result<(), Socks5ConnectionError> {
        if !self.handshake().await? {
            return Ok(());
        }

        let socks5_req = Request::read_from(&mut self.stream).await?;

        let (req, res_receiver) =
            ConnectionRequest::new(socks5_req.command.into(), socks5_req.address.into());

        if let (true, Ok(res)) = (
            self.request_sender.send(req).await.is_ok(),
            res_receiver.await,
        ) {
            match res {
                Ok((mut remote_send, mut remote_recv)) => {
                    let socks5_res =
                        Response::new(Reply::Succeeded, SocketAddr::from(([0, 0, 0, 0], 0)).into());
                    socks5_res.write_to(&mut self.stream).await?;

                    self.forward(&mut remote_send, &mut remote_recv).await;
                }
                Err(err) => {
                    let reply = match err {
                        TuicConnectionError::Tuic(err) => Socks5Error::from(err).as_reply(),
                        _ => Reply::GeneralFailure,
                    };

                    let socks5_res =
                        Response::new(reply, SocketAddr::from(([0, 0, 0, 0], 0)).into());
                    socks5_res.write_to(&mut self.stream).await?;
                }
            }

            Ok(())
        } else {
            let socks5_res = Response::new(
                Reply::GeneralFailure,
                SocketAddr::from(([0, 0, 0, 0], 0)).into(),
            );
            socks5_res.write_to(&mut self.stream).await?;

            Err(Socks5ConnectionError::ConnectionManager)
        }
    }

    async fn handshake(&mut self) -> Result<bool, Socks5Error> {
        let hs_req = HandshakeRequest::read_from(&mut self.stream).await?;
        if hs_req.methods.contains(&handshake::SOCKS5_AUTH_METHOD_NONE) {
            let hs_res = HandshakeResponse::new(handshake::SOCKS5_AUTH_METHOD_NONE);
            hs_res.write_to(&mut self.stream).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn forward(
        &mut self,
        remote_send: &mut QuinnSendStream,
        remote_recv: &mut QuinnRecvStream,
    ) {
        let (mut local_recv, mut local_send) = self.stream.split();
        let remote_to_local = io::copy(remote_recv, &mut local_send);
        let local_to_remote = io::copy(&mut local_recv, remote_send);
        let _ = tokio::try_join!(remote_to_local, local_to_remote);
    }
}

#[derive(Debug, Error)]
pub enum Socks5ConnectionError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Socks5(#[from] Socks5Error),
    #[error("Failed to communicate with the connection manager")]
    ConnectionManager,
}
