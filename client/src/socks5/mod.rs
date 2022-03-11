use crate::client::{Address as RelayAddress, Request as RelayRequest};
use anyhow::{bail, Result};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io,
    net::{TcpListener, TcpStream},
    sync::mpsc::Sender as MpscSender,
};

pub use self::auth::Authentication;
use self::protocol::{
    handshake::password::{Request as PasswordAuthRequest, Response as PasswordAuthResponse},
    Address, HandshakeMethod, HandshakeRequest, HandshakeResponse, Reply, Request, Response,
};

mod auth;
mod protocol;

pub struct Socks5Server {
    listener: TcpListener,
    auth: Arc<Authentication>,
    req_tx: MpscSender<RelayRequest>,
}

impl Socks5Server {
    pub async fn init(
        local_addr: SocketAddr,
        auth: Authentication,
        req_tx: MpscSender<RelayRequest>,
    ) -> Result<Self> {
        let listener = TcpListener::bind(local_addr).await?;
        let auth = Arc::new(auth);

        Ok(Self {
            listener,
            auth,
            req_tx,
        })
    }

    pub async fn run(self) {
        while let Ok((stream, _)) = self.listener.accept().await {
            let conn = Connection::new(stream, &self.auth, &self.req_tx);

            tokio::spawn(async move {
                match conn.process().await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                }
            });
        }
    }
}

struct Connection {
    stream: TcpStream,
    auth: Arc<Authentication>,
    req_tx: MpscSender<RelayRequest>,
}

impl Connection {
    fn new(
        stream: TcpStream,
        auth: &Arc<Authentication>,
        req_tx: &MpscSender<RelayRequest>,
    ) -> Self {
        Self {
            stream,
            auth: auth.clone(),
            req_tx: req_tx.clone(),
        }
    }

    async fn process(mut self) -> Result<()> {
        self.handshake().await?;

        let req = Request::read_from(&mut self.stream).await?;

        match req.command {
            protocol::Command::Connect => self.handle_connect(req.address).await?,
            protocol::Command::Bind => self.handle_bind(req.address).await?,
            protocol::Command::Associate => self.handle_associate(req.address).await?,
        }

        Ok(())
    }

    async fn handshake(&mut self) -> Result<()> {
        let method = self.auth.as_handshake_method();
        let req = HandshakeRequest::read_from(&mut self.stream).await?;

        if req.methods.contains(&method) {
            let resp = HandshakeResponse::new(method);
            resp.write_to(&mut self.stream).await?;

            match self.auth.as_ref() {
                Authentication::None => {}
                Authentication::Gssapi => unimplemented!(),
                Authentication::Password { username, password } => {
                    let req = PasswordAuthRequest::read_from(&mut self.stream).await?;

                    if (&req.username, &req.password) == (username, password) {
                        let resp = PasswordAuthResponse::new(true);
                        resp.write_to(&mut self.stream).await?;
                    } else {
                        let resp = PasswordAuthResponse::new(false);
                        resp.write_to(&mut self.stream).await?;
                        bail!("Password authentication failed");
                    }
                }
            }
        } else {
            let resp = HandshakeResponse::new(HandshakeMethod::Unacceptable);
            resp.write_to(&mut self.stream).await?;
            bail!("No acceptable authentication method");
        }

        Ok(())
    }

    async fn handle_connect(mut self, addr: Address) -> Result<()> {
        let addr = match addr {
            Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
            Address::HostnameAddress(hostname, port) => {
                RelayAddress::HostnameAddress(hostname, port)
            }
        };

        let (relay_req, relay_resp_rx) = RelayRequest::new_connect(addr);
        let _ = self.req_tx.send(relay_req).await;

        let relay_resp = relay_resp_rx.await?;

        if let Some((mut remote_send, mut remote_recv)) = relay_resp {
            let resp = Response::new(
                Reply::Succeeded,
                Address::SocketAddress(SocketAddr::from(([0, 0, 0, 0], 0))),
            );

            resp.write_to(&mut self.stream).await?;

            let (mut local_recv, mut local_send) = self.stream.split();
            let remote_to_local = io::copy(&mut remote_recv, &mut local_send);
            let local_to_remote = io::copy(&mut local_recv, &mut remote_send);
            let _ = tokio::try_join!(remote_to_local, local_to_remote);
        } else {
            let resp = Response::new(
                Reply::GeneralFailure,
                Address::SocketAddress(SocketAddr::from(([0, 0, 0, 0], 0))),
            );

            resp.write_to(&mut self.stream).await?;
        }

        Ok(())
    }

    async fn handle_bind(self, _addr: Address) -> Result<()> {
        Ok(())
    }

    async fn handle_associate(self, addr: Address) -> Result<()> {
        let addr = match addr {
            Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
            Address::HostnameAddress(hostname, port) => {
                RelayAddress::HostnameAddress(hostname, port)
            }
        };

        let (relay_req, pkt_send_tx, pkt_receive_rx) = RelayRequest::new_associate();

        todo!();

        Ok(())
    }
}
