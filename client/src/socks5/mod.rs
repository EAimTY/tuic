use self::protocol::{
    handshake::{
        password::{
            Request as PasswordAuthenticationRequest, Response as PasswordAuthenticationResponse,
        },
        Authentication,
    },
    Error as Socks5Error, HandshakeRequest, HandshakeResponse, Reply, Request, Response,
};
use crate::{
    config::Socks5AuthenticationConfig,
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
    request_sender: MpscSender<ConnectionRequest>,
    local_addr: SocketAddr,
    authentication: Arc<Authentication>,
}

impl Socks5Server {
    pub fn new(config: Config, sender: MpscSender<ConnectionRequest>) -> Self {
        let auth = match config.socks5_authentication {
            Socks5AuthenticationConfig::None => Authentication::None,
            Socks5AuthenticationConfig::Password { username, password } => {
                Authentication::Password { username, password }
            }
        };

        Self {
            request_sender: sender,
            local_addr: config.local_addr,
            authentication: Arc::new(auth),
        }
    }

    pub async fn run(self) -> Result<(), ClientError> {
        let socks5_listener = TcpListener::bind(self.local_addr).await?;

        while let Ok((stream, source_addr)) = socks5_listener.accept().await {
            let mut socks5_conn = Socks5Connection::new(
                stream,
                source_addr,
                &self.request_sender,
                &self.authentication,
            );

            tokio::spawn(async move {
                if let Err(err) = socks5_conn.process().await {
                    log::debug!("{err}");
                }
            });
        }

        Ok(())
    }
}

struct Socks5Connection {
    stream: TcpStream,
    source_addr: SocketAddr,
    request_sender: MpscSender<ConnectionRequest>,
    authentication: Arc<Authentication>,
}

impl Socks5Connection {
    fn new(
        stream: TcpStream,
        source_addr: SocketAddr,
        request_sender: &MpscSender<ConnectionRequest>,
        authentication: &Arc<Authentication>,
    ) -> Self {
        Self {
            stream,
            source_addr,
            request_sender: request_sender.clone(),
            authentication: authentication.clone(),
        }
    }

    async fn process(&mut self) -> Result<(), Socks5ConnectionError> {
        if !self.handshake().await? {
            log::info!("[local][denied]{}", self.source_addr);
            return Ok(());
        }

        let socks5_req = Request::read_from(&mut self.stream).await?;

        log::info!("[local][accepted]{} {:?}", self.source_addr, &socks5_req);

        let (req, res_receiver) =
            ConnectionRequest::new(socks5_req.command.into(), socks5_req.address.into());

        if self.request_sender.send(req).await.is_ok() {
            match res_receiver.await {
                Ok(Ok((remote_send, remote_recv))) => {
                    match socks5_req.command {
                        protocol::Command::Connect => {
                            self.handle_connect(remote_send, remote_recv).await?
                        }
                        protocol::Command::Associate => {
                            self.handle_associate(remote_send, remote_recv).await?
                        }
                    }

                    return Ok(());
                }
                Ok(Err(err)) => {
                    let reply = match err {
                        TuicConnectionError::Tuic(err) => Socks5Error::from(err).as_reply(),
                        _ => Reply::GeneralFailure,
                    };

                    let socks5_res =
                        Response::new(reply, SocketAddr::from(([0, 0, 0, 0], 0)).into());
                    socks5_res.write_to(&mut self.stream).await?;

                    return Ok(());
                }
                _ => {}
            }
        }

        let socks5_res = Response::new(
            Reply::GeneralFailure,
            SocketAddr::from(([0, 0, 0, 0], 0)).into(),
        );
        socks5_res.write_to(&mut self.stream).await?;

        Err(Socks5ConnectionError::ConnectionManager)
    }

    async fn handshake(&mut self) -> Result<bool, Socks5Error> {
        let chosen_method = self.authentication.as_u8();
        let hs_req = HandshakeRequest::read_from(&mut self.stream).await?;

        if hs_req.methods.contains(&chosen_method) {
            let hs_res = HandshakeResponse::new(chosen_method);
            hs_res.write_to(&mut self.stream).await?;

            match self.authentication.as_ref() {
                Authentication::None => Ok(true),
                Authentication::Password { username, password } => {
                    let pwd_req =
                        PasswordAuthenticationRequest::read_from(&mut self.stream).await?;

                    if &pwd_req.username == username && &pwd_req.password == password {
                        let pwd_res = PasswordAuthenticationResponse::new(true);
                        pwd_res.write_to(&mut self.stream).await?;

                        Ok(true)
                    } else {
                        let pwd_res = PasswordAuthenticationResponse::new(false);
                        pwd_res.write_to(&mut self.stream).await?;

                        Ok(false)
                    }
                }
                Authentication::Unacceptable => unreachable!(),
            }
        } else {
            let hs_res = HandshakeResponse::new(Authentication::Unacceptable.as_u8());
            hs_res.write_to(&mut self.stream).await?;

            Ok(false)
        }
    }

    async fn handle_connect(
        &mut self,
        mut remote_send: QuinnSendStream,
        mut remote_recv: QuinnRecvStream,
    ) -> Result<(), Socks5Error> {
        let socks5_res =
            Response::new(Reply::Succeeded, SocketAddr::from(([0, 0, 0, 0], 0)).into());
        socks5_res.write_to(&mut self.stream).await?;

        let (mut local_recv, mut local_send) = self.stream.split();
        let remote_to_local = io::copy(&mut remote_recv, &mut local_send);
        let local_to_remote = io::copy(&mut local_recv, &mut remote_send);
        let _ = tokio::try_join!(remote_to_local, local_to_remote);

        Ok(())
    }

    async fn handle_associate(
        &mut self,
        mut _remote_send: QuinnSendStream,
        mut _remote_recv: QuinnRecvStream,
    ) -> Result<(), Socks5Error> {
        todo!()
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
