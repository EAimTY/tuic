use super::{
    protocol::{
        handshake::password::{Request as PasswordAuthRequest, Response as PasswordAuthResponse},
        Address, Command, Error as ProtocolError, HandshakeMethod, HandshakeRequest,
        HandshakeResponse, Reply, Request, Response,
    },
    Authentication,
};
use crate::relay::Request as RelayRequest;
use anyhow::{bail, Result};
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpStream, sync::mpsc::Sender as MpscSender};

mod associate;
mod bind;
mod connect;

pub struct Connection {
    stream: TcpStream,
    auth: Arc<Authentication>,
    req_tx: MpscSender<RelayRequest>,
}

impl Connection {
    pub async fn handle(
        conn: TcpStream,
        auth: Arc<Authentication>,
        req_tx: MpscSender<RelayRequest>,
    ) -> Result<()> {
        let mut conn = Self {
            stream: conn,
            auth,
            req_tx,
        };

        conn.handshake().await?;

        match Request::read_from(&mut conn.stream).await {
            Ok(req) => match req.command {
                Command::Connect => conn.handle_connect(req.address).await?,
                Command::Bind => conn.handle_bind(req.address).await?,
                Command::Associate => conn.handle_associate(req.address).await?,
            },
            Err(err) => {
                let reply = match err {
                    ProtocolError::Io(err) => Err(err)?,
                    err => {
                        eprintln!("{err}");
                        match err {
                            ProtocolError::UnsupportedCommand(_) => Reply::CommandNotSupported,
                            ProtocolError::UnsupportedAddressType(_)
                            | ProtocolError::AddressInvalidEncoding => {
                                Reply::AddressTypeNotSupported
                            }
                            _ => Reply::GeneralFailure,
                        }
                    }
                };

                let resp = Response::new(
                    reply,
                    Address::SocketAddress(SocketAddr::from(([0, 0, 0, 0], 0))),
                );
                resp.write_to(&mut conn.stream).await?;
            }
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
                Authentication::Gssapi => todo!(),
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
}
