use crate::{certificate, config::ServerAddr, ClientError, Config};
use quinn::{
    ClientConfig as QuinnClientConfig, Connection, ConnectionError as QuinnConnectionError,
    Endpoint, NewConnection, RecvStream, SendStream, WriteError as QuinnWriteError,
};
use rustls::RootCertStore;
use std::{
    io::Error as IoError,
    net::{SocketAddr, ToSocketAddrs},
};
use thiserror::Error;
use tokio::sync::{
    mpsc::{self, Receiver as MpscReceiver, Sender as MpscSender},
    oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
};
use tuic_protocol::{Address, Command, Error as TuicError, Reply, Request, Response};

pub struct ConnectionGuard {
    client_endpoint: Endpoint,
    request_receiver: MpscReceiver<ConnectionRequest>,
    server_addr: ServerAddr,
    token: u64,
    number_of_retries: usize,
}

impl ConnectionGuard {
    pub fn new(config: &Config) -> Result<(Self, MpscSender<ConnectionRequest>), ClientError> {
        let quinn_client_config = load_client_config()?;

        let endpoint = {
            let mut endpoint = Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
            endpoint.set_default_client_config(quinn_client_config);
            endpoint
        };

        let (req_sender, req_receiver) = mpsc::channel(32);

        Ok((
            Self {
                client_endpoint: endpoint,
                request_receiver: req_receiver,
                server_addr: config.server_addr.to_owned(),
                token: config.token,
                number_of_retries: config.number_of_retries,
            },
            req_sender,
        ))
    }

    pub async fn run(mut self) {
        tokio::spawn(async move {
            let mut conn = None;

            while let Some(req) = self.request_receiver.recv().await {
                let (tuic_req, conn_sender) = req.to_tuic_request(self.token);

                let (mut send, mut recv) = match self.get_stream(&mut conn).await {
                    Ok(res) => res,
                    Err(err) => {
                        if let Err(_err) = conn_sender.send(Err(err)) {}
                        continue;
                    }
                };

                async fn handshake(
                    req: Request,
                    send: &mut SendStream,
                    recv: &mut RecvStream,
                ) -> Result<(), ConnectionError> {
                    if let Err(err) = req.write_to(send).await {
                        return Err(err.into());
                    }

                    match Response::read_from(recv).await {
                        Ok(res) => match res.reply {
                            Reply::Succeeded => Ok(()),
                            err => Err(TuicError::from(err).into()),
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

    async fn get_connection(&self) -> Result<Connection, ConnectionError> {
        match &self.server_addr {
            ServerAddr::SocketAddr {
                server_addr,
                server_name,
            } => {
                for _ in 0..=self.number_of_retries {
                    match self.client_endpoint.connect(*server_addr, &server_name) {
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
            ServerAddr::UriAuthorityAddr {
                uri_authority,
                server_port,
            } => {
                for _ in 0..=self.number_of_retries {
                    if let Ok(socket_addrs) =
                        (uri_authority.as_str(), *server_port).to_socket_addrs()
                    {
                        for socket_addr in socket_addrs {
                            match self.client_endpoint.connect(socket_addr, &uri_authority) {
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
            }
        }

        Err(ConnectionError::TooManyRetries(self.number_of_retries))
    }

    async fn get_stream(
        &self,
        conn: &mut Option<Connection>,
    ) -> Result<(SendStream, RecvStream), ConnectionError> {
        if let Some(conn) = conn {
            if let Ok(res) = conn.open_bi().await {
                return Ok(res);
            }
        }

        let err = match self.get_connection().await {
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

pub type ConnectionResponse = Result<(SendStream, RecvStream), ConnectionError>;

pub struct ConnectionRequest {
    command: Command,
    address: Address,
    response_sender: OneshotSender<ConnectionResponse>,
}

impl ConnectionRequest {
    pub fn new(cmd: Command, addr: Address) -> (Self, OneshotReceiver<ConnectionResponse>) {
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

    fn to_tuic_request(self, token: u64) -> (Request, OneshotSender<ConnectionResponse>) {
        let req = Request::new(self.command, token, self.address);
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
    QuicConnection(#[from] QuinnConnectionError),
    #[error(transparent)]
    StreamWrite(#[from] IoError),
    #[error(transparent)]
    StreamClose(#[from] QuinnWriteError),
    #[error(transparent)]
    Tuic(#[from] TuicError),
    #[error("Failed to connect to the server after {0} retries")]
    TooManyRetries(usize),
}
