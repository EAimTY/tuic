use crate::{certificate, client::Error, Config};
use quinn::{ClientConfig as QuinnClientConfig, Endpoint, NewConnection, OpenBi};
use rustls::RootCertStore;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub struct ConnectionManager {
    endpoint: Endpoint,
    channel: mpsc::Receiver<ChannelMessage>,
}

impl ConnectionManager {
    pub fn new(_config: &Config) -> Result<(Self, mpsc::Sender<ChannelMessage>), Error> {
        let quinn_client_config = load_client_config()?;

        let endpoint = {
            let mut endpoint = Endpoint::client(([0, 0, 0, 0], 0).into())?;
            endpoint.set_default_client_config(quinn_client_config);
            endpoint
        };

        let (sender, receiver) = mpsc::channel(8);

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
            let mut conn = self.connect("localhost", 5000).await.unwrap();

            while let Some(msg) = self.channel.recv().await {
                match msg {
                    ChannelMessage::GetConnection(conn_sender) => {
                        let conn = conn.clone();
                        conn_sender.send(conn).map_err(|_| ()).unwrap();
                    }
                    ChannelMessage::ConnectionClosed(closed_conn_id) => {
                        if conn.id() == closed_conn_id {
                            conn = self.connect("localhost", 5000).await.unwrap()
                        }
                    }
                }
            }
        });
    }

    async fn connect(&self, server_addr: &str, server_port: u16) -> Result<Connection, Error> {
        /*let socket_addr = net::lookup_host((server_addr, server_port))
        .await
        .unwrap()
        .next()
        .unwrap();*/
        let socket_addr = ([127, 0, 0, 1], server_port).into();

        let conn = self
            .endpoint
            .connect(socket_addr, server_addr)
            .unwrap()
            .await
            .unwrap();

        Ok(Connection::new(conn))
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

    pub async fn handshake(&self) -> Result<(), Error> {
        let (mut auth_send, mut auth_recv) = self.inner.connection.open_bi().await.unwrap();

        let tuic_hs_req = tuic_protocol::HandshakeRequest::new(Vec::new());
        tuic_hs_req.write_to(&mut auth_send).await?;
        auth_send.finish().await.unwrap();

        let tuic_hs_res = tuic_protocol::HandshakeResponse::read_from(&mut auth_recv)
            .await
            .unwrap();
        if !tuic_hs_res.is_succeeded {
            return Err(Error::AuthFailed);
        }

        Ok(())
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

fn load_client_config() -> Result<QuinnClientConfig, Error> {
    let cert = certificate::load_cert()?;

    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add(&cert).unwrap();

    let client_config = QuinnClientConfig::with_root_certificates(root_cert_store);

    Ok(client_config)
}
