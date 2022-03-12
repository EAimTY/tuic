use super::{
    protocol::{
        handshake::password::{Request as PasswordAuthRequest, Response as PasswordAuthResponse},
        Command, HandshakeMethod, HandshakeRequest, HandshakeResponse, Request,
    },
    Authentication,
};
use crate::relay::Request as RelayRequest;
use anyhow::{bail, Result};
use std::sync::Arc;
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
    pub async fn handle_stream(
        stream: TcpStream,
        auth: Arc<Authentication>,
        req_tx: MpscSender<RelayRequest>,
    ) {
        let conn = Connection {
            stream,
            auth,
            req_tx,
        };

        match conn.process().await {
            Ok(()) => (),
            Err(err) => eprintln!("{err}"),
        }
    }

    async fn process(mut self) -> Result<()> {
        self.handshake().await?;

        let req = Request::read_from(&mut self.stream).await?;

        match req.command {
            Command::Connect => self.handle_connect(req.address).await?,
            Command::Bind => self.handle_bind(req.address).await?,
            Command::Associate => self.handle_associate(req.address).await?,
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
}
