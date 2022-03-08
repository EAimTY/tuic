use crate::config::ServerAddr;
use anyhow::Result;
use blake3::Hash;
use quinn::{
    ClientConfig, Connection as QuinnConnection, ConnectionError, Datagrams as QuinnDatagrams,
    Endpoint, IncomingUniStreams as QuinnUniStreams, NewConnection, RecvStream, SendStream,
};
use rustls::{Certificate, RootCertStore};
use std::{
    mem::{self, MaybeUninit},
    net::{SocketAddr, ToSocketAddrs},
    vec::IntoIter,
};
use tokio::sync::{
    mpsc::{self, Receiver as MpscReceiver, Sender as MpscSender},
    oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
};
use tuic_protocol::{Address as TuicAddress, Command as TuicCommand, Response as TuicResponse};

pub struct TuicClient {
    endpoint: Endpoint,
    server_addr: ServerAddr,
    token: Hash,
    req_rx: MpscReceiver<Request>,
}

impl TuicClient {
    pub fn init(
        server_addr: ServerAddr,
        certificate: Option<Certificate>,
        token: Hash,
    ) -> Result<(Self, MpscSender<Request>)> {
        let quinn_config = if let Some(cert) = certificate {
            let mut root_cert_store = RootCertStore::empty();
            root_cert_store.add(&cert)?;
            ClientConfig::with_root_certificates(root_cert_store)
        } else {
            ClientConfig::with_native_roots()
        };

        let mut endpoint = Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
        endpoint.set_default_client_config(quinn_config);

        let (req_tx, req_rx) = mpsc::channel(1);

        Ok((
            Self {
                endpoint,
                server_addr,
                token,
                req_rx,
            },
            req_tx,
        ))
    }

    pub async fn run(mut self) {
        let mut conn = self.establish_conn().await;

        while let Some(req) = self.req_rx.recv().await {
            match req {
                Request::Connect { addr, tx } => {
                    let (send, recv) = self.get_bi_stream(&mut conn).await;
                    tokio::spawn(handle_connect(send, recv, addr, tx));
                }
            }
        }
    }

    async fn get_bi_stream(&self, conn_ref: &mut Connection) -> (SendStream, RecvStream) {
        loop {
            match conn_ref.conn.open_bi().await {
                Ok(res) => return res,
                Err(err) => {
                    match err {
                        ConnectionError::ConnectionClosed(_) | ConnectionError::TimedOut => {}
                        err => eprintln!("{err}"),
                    }
                    *conn_ref = self.establish_conn().await
                }
            }
        }
    }

    async fn establish_conn(&self) -> Connection {
        let (mut addrs, server_name) = match &self.server_addr {
            ServerAddr::HostnameAddr {
                hostname,
                server_port,
            } => (
                unsafe { mem::transmute(MaybeUninit::<IntoIter<SocketAddr>>::uninit()) },
                hostname,
            ),
            ServerAddr::SocketAddr {
                server_addr,
                server_name,
            } => (vec![server_addr.to_owned()].into_iter(), server_name),
        };

        loop {
            if let ServerAddr::HostnameAddr {
                hostname,
                server_port,
            } = &self.server_addr
            {
                match (hostname.as_str(), *server_port).to_socket_addrs() {
                    Ok(resolved) => addrs = resolved,
                    Err(err) => {
                        eprintln!("{err}");
                        continue;
                    }
                }
            }

            for addr in addrs.as_ref() {
                match self.endpoint.connect(*addr, server_name) {
                    Ok(connecting) => match connecting.await {
                        Ok(NewConnection {
                            connection: conn,
                            uni_streams,
                            datagrams,
                            ..
                        }) => return Connection::new(&self.token, conn, uni_streams, datagrams),
                        Err(err) => eprintln!("{err}"),
                    },
                    Err(err) => eprintln!("{err}"),
                }
            }
        }
    }
}

pub struct Connection {
    conn: QuinnConnection,
    uni_streams: QuinnUniStreams,
    datagrams: QuinnDatagrams,
}

impl Connection {
    fn new(
        token: &Hash,
        conn: QuinnConnection,
        uni_streams: QuinnUniStreams,
        datagrams: QuinnDatagrams,
    ) -> Self {
        let token = TuicCommand::new_authenticate(token.as_bytes().to_owned());
        let uni = conn.open_uni();

        tokio::spawn(async move {
            match uni.await {
                Ok(mut stream) => match token.write_to(&mut stream).await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                },
                Err(err) => eprintln!("{err}"),
            }
        });

        Self {
            conn,
            uni_streams,
            datagrams,
        }
    }
}

pub enum Request {
    Connect {
        addr: Address,
        tx: OneshotSender<Option<(SendStream, RecvStream)>>,
    },
}

impl Request {
    pub fn new_connect(addr: Address) -> (Self, OneshotReceiver<Option<(SendStream, RecvStream)>>) {
        let (tx, rx) = oneshot::channel();
        (Request::Connect { addr, tx }, rx)
    }
}

pub enum Address {
    HostnameAddress(String, u16),
    SocketAddress(SocketAddr),
}

async fn handle_connect(
    mut send: SendStream,
    mut recv: RecvStream,
    addr: Address,
    tx: OneshotSender<Option<(SendStream, RecvStream)>>,
) {
    let addr = match addr {
        Address::HostnameAddress(hostname, port) => TuicAddress::HostnameAddress(hostname, port),
        Address::SocketAddress(addr) => TuicAddress::SocketAddress(addr),
    };

    let cmd = TuicCommand::new_connect(addr);

    match cmd.write_to(&mut send).await {
        Ok(()) => match TuicResponse::read_from(&mut recv).await {
            Ok(res) => {
                if res.is_succeeded() {
                    let _ = tx.send(Some((send, recv)));
                    return;
                }
            }
            Err(err) => eprintln!("{err}"),
        },
        Err(err) => eprintln!("{err}"),
    }
    let _ = tx.send(None);
}
