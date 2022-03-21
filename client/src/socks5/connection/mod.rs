use super::{
    protocol::{
        handshake::password::{Request as PasswordAuthRequest, Response as PasswordAuthResponse},
        Address, Command, Error as ProtocolError, HandshakeMethod, HandshakeRequest,
        HandshakeResponse, Reply, Request, Response,
    },
    Authentication, Socks5Error,
};
use crate::relay::Request as RelayRequest;
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpStream, sync::mpsc::Sender};

mod associate;
mod bind;
mod connect;

pub struct Connection {
    stream: TcpStream,
    auth: Arc<Authentication>,
    req_tx: Sender<RelayRequest>,
}

impl Connection {
    pub async fn handle(
        conn: TcpStream,
        src_addr: SocketAddr,
        auth: Arc<Authentication>,
        max_udp_pkt_size: usize,
        req_tx: Sender<RelayRequest>,
    ) -> Result<(), Socks5Error> {
        log::debug!("[socks5] [{src_addr}] [establish]");

        let mut conn = Self {
            stream: conn,
            auth,
            req_tx,
        };

        conn.handshake().await?;
        log::debug!("[socks5] [{src_addr}] [handshake]");

        match Request::read_from(&mut conn.stream).await {
            Ok(req) => match req.command {
                Command::Connect => {
                    log::info!("[socks5] [{src_addr}] [connect] [{}]", req.address);
                    conn.handle_connect(req.address).await?
                }
                Command::Bind => {
                    log::info!("[socks5] [{src_addr}] [bind] [{}]", req.address);
                    conn.handle_bind(req.address).await?
                }
                Command::Associate => {
                    let req_addr = req.address.to_string();
                    log::info!("[socks5] [{src_addr}] [associate] [{req_addr}]");

                    conn.handle_associate(src_addr, max_udp_pkt_size).await?;

                    log::info!("[socks5] [{src_addr}] [dissociate] [{req_addr}]");
                }
            },
            Err(ProtocolError::Io(err)) => return Err(Socks5Error::Io(err)),
            Err(err) => {
                let reply = match &err {
                    ProtocolError::UnsupportedCommand(_) => Reply::CommandNotSupported,
                    ProtocolError::UnsupportedAddressType(_)
                    | ProtocolError::AddressInvalidEncoding => Reply::AddressTypeNotSupported,
                    _ => Reply::GeneralFailure,
                };

                let resp = Response::new(
                    reply,
                    Address::SocketAddress(SocketAddr::from(([0, 0, 0, 0], 0))),
                );
                resp.write_to(&mut conn.stream).await?;

                return Err(Socks5Error::Protocol(err));
            }
        }

        log::debug!("[socks5] [{src_addr}] [disconnect]");

        Ok(())
    }

    async fn handshake(&mut self) -> Result<(), Socks5Error> {
        let method = self.auth.as_handshake_method();
        let req = HandshakeRequest::read_from(&mut self.stream).await?;

        if req.methods.contains(&method) {
            let resp = HandshakeResponse::new(method);
            resp.write_to(&mut self.stream).await?;

            match self.auth.as_ref() {
                Authentication::None => {}
                Authentication::Gssapi => todo!(),
                Authentication::Password { username, password } => {
                    let req = PasswordAuthRequest::read_from(&mut self.stream).await?;

                    if (&req.username, &req.password) == (username, password) {
                        let resp = PasswordAuthResponse::new(true);
                        resp.write_to(&mut self.stream).await?;
                    } else {
                        let resp = PasswordAuthResponse::new(false);
                        resp.write_to(&mut self.stream).await?;
                        return Err(Socks5Error::Authentication);
                    }
                }
            }
        } else {
            let resp = HandshakeResponse::new(HandshakeMethod::Unacceptable);
            resp.write_to(&mut self.stream).await?;
            return Err(Socks5Error::Authentication);
        }

        Ok(())
    }
}
