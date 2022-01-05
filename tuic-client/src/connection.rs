use crate::{certificate, exit, ClientError, Config};
use quinn::{ClientConfig as QuinnClientConfig, Endpoint, NewConnection, OpenBi};
use rustls::RootCertStore;
use std::{io, net::ToSocketAddrs, sync::Arc};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

pub struct ConnectionManager {
    endpoint: Endpoint,
    channel: mpsc::Receiver<ChannelMessage>,
}

impl ConnectionManager {
    pub fn new(_config: &Config) -> Result<(Self, mpsc::Sender<ChannelMessage>), ClientError> {
        let quinn_client_config = load_client_config()?;

        let endpoint = {
            let mut endpoint = Endpoint::client(([0, 0, 0, 0], 0).into())?;
            endpoint.set_default_client_config(quinn_client_config);
            endpoint
        };

        let (sender, receiver) = mpsc::channel(32);

        Ok((
            Self {
                endpoint,
                channel: receiver,
            },
            sender,
        ))
    }

    pub async fn run(mut self) {
        tokio::spawn(async move {
            let mut conn = loop {
                if let Ok(conn) = self.connect("localhost", 5000).await {
                    break conn;
                }
            };

            while let Some(msg) = self.channel.recv().await {
                match msg {
                    ChannelMessage::GetConnection(conn_sender) => {
                        let conn = conn.clone();
                        if conn_sender.send(conn).is_err() {
                            exit(ClientError::GetConnection.into());
                        }
                    }
                    ChannelMessage::ConnectionClosed(closed_conn_id) => {
                        if conn.id() == closed_conn_id {
                            conn = match self.connect("localhost", 5000).await {
                                Ok(conn) => conn,
                                Err(err) => exit(err.into()),
                            };
                        }
                    }
                }
            }
        });
    }

    async fn connect(
        &self,
        server_addr: &str,
        server_port: u16,
    ) -> Result<Connection, ConnectionError> {
        let mut retries = 0;

        while retries <= 5 {
            retries += 1;

            if let Ok(socket_addrs) = (server_addr, server_port).to_socket_addrs() {
                for socket_addr in socket_addrs {
                    match self.endpoint.connect(socket_addr, server_addr) {
                        Ok(connecting) => match connecting.await {
                            Ok(conn) => return Ok(Connection::new(conn)),
                            Err(_err) => {}
                        },
                        Err(_err) => {}
                    }
                }
            }
        }

        Err(ConnectionError::TooManyRetries)
    }
}

#[derive(Clone)]
pub struct Connection {
    inner: Arc<NewConnection>,
}

impl Connection {
    fn new(inner: NewConnection) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    fn id(&self) -> usize {
        self.inner.connection.stable_id()
    }

    pub fn open_bi(&self) -> OpenBi {
        self.inner.connection.open_bi()
    }

    pub async fn handshake(&self) -> Result<bool, ConnectionError> {
        let (mut hs_send, mut hs_recv) = self.inner.connection.open_bi().await?;

        let hs_req = tuic_protocol::HandshakeRequest::new(Vec::new());
        hs_req.write_to(&mut hs_send).await?;
        hs_send.finish().await?;

        let hs_res = tuic_protocol::HandshakeResponse::read_from(&mut hs_recv).await?;

        Ok(hs_res.is_succeeded())
    }
}

pub enum ChannelMessage {
    GetConnection(oneshot::Sender<Connection>),
    ConnectionClosed(usize),
}

impl ChannelMessage {
    pub fn get_connection() -> (Self, oneshot::Receiver<Connection>) {
        let (sender, receiver) = oneshot::channel();
        (Self::GetConnection(sender), receiver)
    }

    pub fn connection_closed(conn_id: usize) -> Self {
        ChannelMessage::ConnectionClosed(conn_id)
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
    TuicProtocol(#[from] tuic_protocol::Error),
    #[error("Failed to connect to the server")]
    TooManyRetries,
}
