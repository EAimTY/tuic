use crate::{certificate, ClientError, Config};
use quinn::{ClientConfig as QuinnClientConfig, Connection, Endpoint, NewConnection};
use rustls::RootCertStore;
use std::{io, net::ToSocketAddrs};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

pub struct ConnectionGuard {
    client_endpoint: Endpoint,
    request_receiver: mpsc::Receiver<ConnectionRequest>,
}

impl ConnectionGuard {
    pub fn new(_config: &Config) -> Result<(Self, mpsc::Sender<ConnectionRequest>), ClientError> {
        let quinn_client_config = load_client_config()?;

        let endpoint = {
            let mut endpoint = Endpoint::client(([0, 0, 0, 0], 0).into())?;
            endpoint.set_default_client_config(quinn_client_config);
            endpoint
        };

        let (req_sender, req_receiver) = mpsc::channel(32);

        Ok((
            Self {
                client_endpoint: endpoint,
                request_receiver: req_receiver,
            },
            req_sender,
        ))
    }

    pub async fn run(mut self) {
        tokio::spawn(async move {
            let mut conn = None;

            while let Some(req) = self.request_receiver.recv().await {
                let (tuic_req, conn_sender) = req.to_tuic_request();

                let (mut send, mut recv) = match self.get_stream(&mut conn).await {
                    Ok(res) => res,
                    Err(err) => {
                        conn_sender.send(Err(err));
                        continue;
                    }
                };

                async fn handshake(
                    req: tuic_protocol::Request,
                    send: &mut quinn::SendStream,
                    recv: &mut quinn::RecvStream,
                ) -> Result<(), ConnectionError> {
                    if let Err(err) = req.write_to(send).await {
                        return Err(err.into());
                    }

                    match tuic_protocol::Response::read_from(recv).await {
                        Ok(res) => match res.reply {
                            tuic_protocol::Reply::Succeeded => Ok(()),
                            err => Err(tuic_protocol::Error::from(err).into()),
                        },
                        Err(err) => Err(err.into()),
                    }
                }

                tokio::spawn(async move {
                    match handshake(tuic_req, &mut send, &mut recv).await {
                        Ok(()) => conn_sender.send(Ok((send, recv))),
                        Err(err) => conn_sender.send(Err(err)),
                    }
                });
            }
        });
    }

    async fn get_connection(
        &self,
        server_addr: &str,
        server_port: u16,
    ) -> Result<Connection, ConnectionError> {
        let mut retries = 0usize;

        while retries <= 5 {
            retries += 1;

            if let Ok(socket_addrs) = (server_addr, server_port).to_socket_addrs() {
                for socket_addr in socket_addrs {
                    match self.client_endpoint.connect(socket_addr, server_addr) {
                        Ok(connecting) => match connecting.await {
                            Ok(NewConnection {
                                connection: conn, ..
                            }) => return Ok(conn),
                            Err(_err) => {}
                        },
                        Err(_err) => {}
                    }
                }
            }
        }

        Err(ConnectionError::TooManyRetries)
    }

    async fn get_stream(
        &self,
        conn: &mut Option<Connection>,
    ) -> Result<(quinn::SendStream, quinn::RecvStream), ConnectionError> {
        if let Some(conn) = conn {
            if let Ok(res) = conn.open_bi().await {
                return Ok(res);
            }
        }

        let err = match self.get_connection("localhost", 5000).await {
            Ok(new_conn) => match new_conn.open_bi().await {
                Ok(res) => {
                    *conn = Some(new_conn);
                    return Ok(res);
                }
                Err(err) => err.into(),
            },
            Err(err) => err,
        };

        *conn = None;

        Err(err)
    }
}

pub type ConnectionResponse = Result<(quinn::SendStream, quinn::RecvStream), ConnectionError>;

pub struct ConnectionRequest {
    command: tuic_protocol::Command,
    address: tuic_protocol::Address,
    response_sender: oneshot::Sender<ConnectionResponse>,
}

impl ConnectionRequest {
    pub fn new(
        cmd: tuic_protocol::Command,
        addr: tuic_protocol::Address,
    ) -> (Self, oneshot::Receiver<ConnectionResponse>) {
        let (res_sender, res_receiver) = oneshot::channel();
        (
            Self {
                command: cmd,
                address: addr,
                response_sender: res_sender,
            },
            res_receiver,
        )
    }

    fn to_tuic_request(self) -> (tuic_protocol::Request, oneshot::Sender<ConnectionResponse>) {
        let req = tuic_protocol::Request::new(self.command, 0, self.address);
        (req, self.response_sender)
    }
}

fn load_client_config() -> Result<QuinnClientConfig, ClientError> {
    let cert = certificate::load_cert()?;

    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add(&cert).unwrap();

    let client_config = QuinnClientConfig::with_root_certificates(root_cert_store);

    Ok(client_config)
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error(transparent)]
    QuicConnection(#[from] quinn::ConnectionError),
    #[error(transparent)]
    StreamWrite(#[from] io::Error),
    #[error(transparent)]
    StreamClose(#[from] quinn::WriteError),
    #[error(transparent)]
    Tuic(#[from] tuic_protocol::Error),
    #[error("Failed to connect to the server")]
    TooManyRetries,
}
