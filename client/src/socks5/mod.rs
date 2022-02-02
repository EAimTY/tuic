use self::protocol::{
    handshake::{
        password::{
            Request as PasswordAuthenticationRequest, Response as PasswordAuthenticationResponse,
        },
        HandshakeMethod,
    },
    Address, Error as Socks5Error, HandshakeRequest, HandshakeResponse, Reply, Request, Response,
};
use crate::{config::Socks5AuthConfig, relay::RelayRequest, Config};
use quinn::{RecvStream as QuinnRecvStream, SendStream as QuinnSendStream};
use std::{io::Error as IoError, net::SocketAddr, sync::Arc};
use thiserror::Error;
use tokio::{
    io,
    net::{TcpListener, TcpStream, UdpSocket},
    sync::mpsc::Sender as MpscSender,
    task::JoinHandle,
};

mod convert;
mod protocol;

pub struct Socks5Server {
    relay_req_tx: MpscSender<RelayRequest>,
    local_addr: SocketAddr,
    auth: Arc<Authentication>,
}

impl Socks5Server {
    pub async fn init(
        config: Config,
        relay_req_tx: MpscSender<RelayRequest>,
    ) -> Result<JoinHandle<()>, IoError> {
        let auth = match config.socks5_auth {
            Socks5AuthConfig::None => Authentication::None,
            Socks5AuthConfig::Gssapi => Authentication::Gssapi,
            Socks5AuthConfig::Password { username, password } => {
                Authentication::Password { username, password }
            }
        };

        let server = Self {
            relay_req_tx,
            local_addr: config.local_addr,
            auth: Arc::new(auth),
        };

        let listener = TcpListener::bind(server.local_addr).await?;

        Ok(tokio::spawn(async move {
            while let Ok((stream, from_addr)) = listener.accept().await {
                let mut socks5_conn =
                    Socks5Connection::new(stream, from_addr, &server.relay_req_tx, &server.auth);

                tokio::spawn(async move {
                    if let Err(err) = socks5_conn.process().await {
                        log::debug!("{err}");
                    }
                });
            }
        }))
    }
}

enum Authentication {
    None,
    Gssapi,
    Password {
        username: Vec<u8>,
        password: Vec<u8>,
    },
}

impl Authentication {
    fn as_handshake_method(&self) -> HandshakeMethod {
        match self {
            Authentication::None => HandshakeMethod::NONE,
            Authentication::Gssapi => HandshakeMethod::GSSAPI,
            Authentication::Password { .. } => HandshakeMethod::PASSWORD,
        }
    }
}

struct Socks5Connection {
    stream: TcpStream,
    from_addr: SocketAddr,
    relay_req_tx: MpscSender<RelayRequest>,
    auth: Arc<Authentication>,
}

impl Socks5Connection {
    fn new(
        stream: TcpStream,
        from_addr: SocketAddr,
        relay_req_tx: &MpscSender<RelayRequest>,
        auth: &Arc<Authentication>,
    ) -> Self {
        Self {
            stream,
            from_addr,
            relay_req_tx: relay_req_tx.clone(),
            auth: auth.clone(),
        }
    }

    async fn process(&mut self) -> Result<(), Socks5ConnectionError> {
        if !self.handshake().await? {
            log::info!("[local][denied]{}", self.from_addr);
            return Ok(());
        }

        let socks5_req = Request::read_from(&mut self.stream).await?;

        log::info!("[local][accepted]{} {:?}", self.from_addr, &socks5_req);

        let err = match socks5_req.command {
            protocol::Command::Connect => {
                let (req, relay_res_rx) = RelayRequest::new_connect(socks5_req.address.into());
                let _ = self.relay_req_tx.send(req).await;

                match unsafe { relay_res_rx.await.unwrap_unchecked() } {
                    Ok((remote_send, remote_recv)) => {
                        self.handle_connect(remote_send, remote_recv).await?;

                        return Ok(());
                    }
                    Err(err) => err,
                }
            }
            protocol::Command::Associate => {
                let (req, _relay_res_rx) = RelayRequest::new_associate();
                let _ = self.relay_req_tx.send(req).await;
                todo!()
            }
        };

        let socks5_res = Response::new(
            Socks5Error::from(err).as_reply(),
            SocketAddr::from(([0, 0, 0, 0], 0)).into(),
        );
        socks5_res.write_to(&mut self.stream).await?;

        Ok(())
    }

    async fn handshake(&mut self) -> Result<bool, Socks5Error> {
        let chosen_method = self.auth.as_handshake_method();
        let hs_req = HandshakeRequest::read_from(&mut self.stream).await?;

        if hs_req.methods.contains(&chosen_method) {
            let hs_res = HandshakeResponse::new(chosen_method);
            hs_res.write_to(&mut self.stream).await?;

            match self.auth.as_ref() {
                Authentication::None => Ok(true),
                Authentication::Gssapi => {
                    let hs_res = HandshakeResponse::new(HandshakeMethod::UNACCEPTABLE);
                    hs_res.write_to(&mut self.stream).await?;
                    Ok(false)
                }
                Authentication::Password { username, password } => {
                    let pwd_req =
                        PasswordAuthenticationRequest::read_from(&mut self.stream).await?;

                    if (&pwd_req.username, &pwd_req.password) == (username, password) {
                        let pwd_res = PasswordAuthenticationResponse::new(true);
                        pwd_res.write_to(&mut self.stream).await?;
                        Ok(true)
                    } else {
                        let pwd_res = PasswordAuthenticationResponse::new(false);
                        pwd_res.write_to(&mut self.stream).await?;
                        Ok(false)
                    }
                }
            }
        } else {
            let hs_res = HandshakeResponse::new(HandshakeMethod::UNACCEPTABLE);
            hs_res.write_to(&mut self.stream).await?;
            Ok(false)
        }
    }

    async fn handle_connect(
        &mut self,
        mut remote_send: QuinnSendStream,
        mut remote_recv: QuinnRecvStream,
    ) -> Result<(), Socks5Error> {
        let (mut local_recv, mut local_send) = self.stream.split();
        let remote_to_local = io::copy(&mut remote_recv, &mut local_send);
        let local_to_remote = io::copy(&mut local_recv, &mut remote_send);
        let _ = tokio::try_join!(remote_to_local, local_to_remote);

        Ok(())
    }

    async fn _handle_associate(
        &mut self,
        _input_addr: Address,
        mut _remote_send: QuinnSendStream,
        mut _remote_recv: QuinnRecvStream,
    ) -> Result<(), Socks5Error> {
        if let Ok(assoc_socket) = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await {
            if let Ok(assoc_addr) = assoc_socket.local_addr() {
                let socks5_res = Response::new(Reply::Succeeded, assoc_addr.into());
                socks5_res.write_to(&mut self.stream).await?;

                return Ok(());
            }
        }

        let socks5_failure_res = Response::new(
            Reply::GeneralFailure,
            SocketAddr::from(([0, 0, 0, 0], 0)).into(),
        );
        socks5_failure_res.write_to(&mut self.stream).await?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum Socks5ConnectionError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Socks5(#[from] Socks5Error),
}
