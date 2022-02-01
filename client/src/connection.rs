use crate::{certificate, config::ServerAddr, ClientError, Config};
use quinn::{
    ClientConfig as QuinnClientConfig, Connection as QuinnConnection, Endpoint, NewConnection,
    RecvStream, SendStream,
};
use rustls::RootCertStore;
use std::net::{SocketAddr, ToSocketAddrs};
use tokio::{
    sync::{
        mpsc::{self, Receiver as MpscReceiver, Sender as MpscSender},
        oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
    },
    task::JoinHandle,
};

pub struct Connection {
    cli_endpoint: Endpoint,
    stream_req_rx: MpscReceiver<GetStreamRequest>,
    server_addr: ServerAddr,
    retries: usize,
}

impl Connection {
    pub fn init(
        config: &Config,
    ) -> Result<(JoinHandle<()>, MpscSender<GetStreamRequest>), ClientError> {
        let quinn_config = load_client_config(config.certificate_path.as_deref())?;

        let cli_endpoint = {
            let mut endpoint = Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
            endpoint.set_default_client_config(quinn_config);
            endpoint
        };

        let (stream_req_tx, stream_req_rx) = mpsc::channel(32);

        let conn = {
            let mut conn = Self {
                cli_endpoint,
                stream_req_rx,
                server_addr: config.server_addr.to_owned(),
                retries: config.number_of_retries,
            };

            tokio::spawn(async move {
                let mut active = None;

                while let Some(GetStreamRequest { stream_res_tx }) = conn.stream_req_rx.recv().await
                {
                    let res = conn.get_stream(&mut active).await;
                    let _ = stream_res_tx.send(res);
                }
            })
        };

        Ok((conn, stream_req_tx))
    }

    async fn get_connection(&self) -> Option<QuinnConnection> {
        match &self.server_addr {
            ServerAddr::SocketAddr {
                server_addr,
                server_name,
            } => {
                for _ in 0..=self.retries {
                    match self.cli_endpoint.connect(*server_addr, server_name) {
                        Ok(connecting) => match connecting.await {
                            Ok(NewConnection {
                                connection: conn, ..
                            }) => return Some(conn),
                            Err(err) => log::warn!("{err}"),
                        },
                        Err(err) => log::warn!("{err}"),
                    }
                }
            }
            ServerAddr::HostnameAddr {
                hostname,
                server_port,
            } => {
                for _ in 0..=self.retries {
                    if let Ok(socket_addrs) = (hostname.as_str(), *server_port).to_socket_addrs() {
                        for socket_addr in socket_addrs {
                            match self.cli_endpoint.connect(socket_addr, hostname) {
                                Ok(connecting) => match connecting.await {
                                    Ok(NewConnection {
                                        connection: conn, ..
                                    }) => return Some(conn),
                                    Err(err) => log::warn!("{err}"),
                                },
                                Err(err) => log::warn!("{err}"),
                            }
                        }
                    }
                }
            }
        }

        None
    }

    async fn get_stream(
        &self,
        conn: &mut Option<QuinnConnection>,
    ) -> Option<(SendStream, RecvStream)> {
        if let Some(conn) = conn {
            if let Ok(res) = conn.open_bi().await {
                return Some(res);
            }
        }

        if let Some(new_conn) = self.get_connection().await {
            if let Ok(res) = new_conn.open_bi().await {
                *conn = Some(new_conn);
                return Some(res);
            }
        }

        *conn = None;

        None
    }
}

pub struct GetStreamRequest {
    stream_res_tx: OneshotSender<GetStreamResponse>,
}

pub type GetStreamResponse = Option<(SendStream, RecvStream)>;

impl GetStreamRequest {
    pub fn new() -> (Self, OneshotReceiver<GetStreamResponse>) {
        let (stream_res_tx, stream_res_rx) = oneshot::channel();
        (Self { stream_res_tx }, stream_res_rx)
    }
}

fn load_client_config(cert_path: Option<&str>) -> Result<QuinnClientConfig, ClientError> {
    if let Some(cert_path) = cert_path {
        let cert = certificate::load_cert(cert_path)?;

        let mut root_cert_store = RootCertStore::empty();
        root_cert_store.add(&cert)?;

        let client_config = QuinnClientConfig::with_root_certificates(root_cert_store);
        Ok(client_config)
    } else {
        let client_config = QuinnClientConfig::with_native_roots();
        Ok(client_config)
    }
}
