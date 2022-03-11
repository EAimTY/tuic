use crate::config::{CongestionController, ServerAddr};
use anyhow::Result;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    ClientConfig, Connecting, Connection, ConnectionError, Datagrams, Endpoint, IncomingUniStreams,
    NewConnection, RecvStream, SendStream, TransportConfig,
};
use rand::{rngs::StdRng, RngCore, SeedableRng};
use rustls::{Certificate, RootCertStore};
use std::{
    mem::{self, MaybeUninit},
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
    vec::IntoIter,
};
use tokio::{
    sync::{
        mpsc::{self, Receiver as MpscReceiver, Sender as MpscSender},
        oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
    },
    task::JoinHandle,
};
use tuic_protocol::{Address as TuicAddress, Command as TuicCommand, Response as TuicResponse};

pub fn init(
    server_addr: ServerAddr,
    certificate: Option<Certificate>,
    token_digest: [u8; 32],
    reduce_rtt: bool,
    congestion_controller: CongestionController,
) -> Result<(JoinHandle<()>, MpscSender<Request>)> {
    let config = {
        let mut config = if let Some(cert) = certificate {
            let mut root_cert_store = RootCertStore::empty();
            root_cert_store.add(&cert)?;
            ClientConfig::with_root_certificates(root_cert_store)
        } else {
            ClientConfig::with_native_roots()
        };

        let mut transport = TransportConfig::default();

        match congestion_controller {
            CongestionController::Cubic => {
                transport.congestion_controller_factory(Arc::new(CubicConfig::default()))
            }
            CongestionController::NewReno => {
                transport.congestion_controller_factory(Arc::new(NewRenoConfig::default()))
            }
            CongestionController::Bbr => {
                transport.congestion_controller_factory(Arc::new(BbrConfig::default()))
            }
        };

        config.transport = Arc::new(transport);
        config
    };

    let mut endpoint = Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
    endpoint.set_default_client_config(config);

    let (req_tx, req_rx) = mpsc::channel(1);

    let listen_relay_request = tokio::spawn(listen_relay_request(
        endpoint,
        server_addr,
        token_digest,
        reduce_rtt,
        req_rx,
    ));

    Ok((listen_relay_request, req_tx))
}

async fn listen_relay_request(
    endpoint: Endpoint,
    server_addr: ServerAddr,
    token_digest: [u8; 32],
    reduce_rtt: bool,
    mut req_rx: MpscReceiver<Request>,
) {
    let mut conn = establish_connection(&endpoint, &server_addr, token_digest, reduce_rtt).await;

    while let Some(req) = req_rx.recv().await {
        match req {
            Request::Connect { addr, tx } => {
                let (send, recv) =
                    new_bi_stream(&mut conn, &endpoint, &server_addr, token_digest, reduce_rtt)
                        .await;
                tokio::spawn(handle_command_connect(send, recv, addr, tx));
            }
            Request::Associate {
                assoc_id,
                pkt_send_rx,
                pkt_receive_tx,
            } => {}
        }
    }
}

async fn establish_connection(
    endpoint: &Endpoint,
    server_addr: &ServerAddr,
    token_digest: [u8; 32],
    reduce_rtt: bool,
) -> (Connection, IncomingUniStreams, Datagrams) {
    let (mut addrs, server_name) = match &server_addr {
        ServerAddr::HostnameAddr { hostname, .. } => (
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
        } = &server_addr
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
            match endpoint.connect(*addr, server_name) {
                Ok(conn) => match process_connection(conn, token_digest, reduce_rtt).await {
                    Ok(conn) => return conn,
                    Err(err) => eprintln!("{err}"),
                },
                Err(err) => eprintln!("{err}"),
            }
        }
    }
}

async fn process_connection(
    conn: Connecting,
    token_digest: [u8; 32],
    reduce_rtt: bool,
) -> Result<(Connection, IncomingUniStreams, Datagrams)> {
    let NewConnection {
        connection,
        uni_streams,
        datagrams,
        ..
    } = if reduce_rtt {
        match conn.into_0rtt() {
            Ok((conn, _)) => conn,
            Err(conn) => conn.await?,
        }
    } else {
        conn.await?
    };

    let token = TuicCommand::new_authenticate(token_digest);
    let uni = connection.open_uni();

    tokio::spawn(async move {
        match uni.await {
            Ok(mut stream) => match token.write_to(&mut stream).await {
                Ok(()) => {}
                Err(err) => eprintln!("{err}"),
            },
            Err(err) => eprintln!("{err}"),
        }
    });

    Ok((connection, uni_streams, datagrams))
}

async fn new_bi_stream(
    conn_ref: &mut (Connection, IncomingUniStreams, Datagrams),
    endpoint: &Endpoint,
    server_addr: &ServerAddr,
    token_digest: [u8; 32],
    reduce_rtt: bool,
) -> (SendStream, RecvStream) {
    loop {
        match conn_ref.0.open_bi().await {
            Ok(res) => return res,
            Err(err) => {
                match err {
                    ConnectionError::ConnectionClosed(_) | ConnectionError::TimedOut => {}
                    err => eprintln!("{err}"),
                }
                *conn_ref =
                    establish_connection(endpoint, server_addr, token_digest, reduce_rtt).await
            }
        }
    }
}

async fn handle_command_connect(
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

fn get_random_u32() -> u32 {
    lazy_static! {
        static ref RNG: Mutex<StdRng> = Mutex::new(StdRng::from_entropy());
    }

    RNG.lock().next_u32()
}

pub enum Request {
    Connect {
        addr: Address,
        tx: OneshotSender<Option<(SendStream, RecvStream)>>,
    },
    Associate {
        assoc_id: u32,
        pkt_send_rx: MpscReceiver<()>,
        pkt_receive_tx: MpscSender<()>,
    },
}

impl Request {
    pub fn new_connect(addr: Address) -> (Self, OneshotReceiver<Option<(SendStream, RecvStream)>>) {
        let (tx, rx) = oneshot::channel();
        (Request::Connect { addr, tx }, rx)
    }

    pub fn new_associate() -> (Self, MpscSender<()>, MpscReceiver<()>) {
        let assoc_id = get_random_u32();
        let (pkt_send_tx, pkt_send_rx) = mpsc::channel(1);
        let (pkt_receive_tx, pkt_receive_rx) = mpsc::channel(1);

        (
            Self::Associate {
                assoc_id,
                pkt_send_rx,
                pkt_receive_tx,
            },
            pkt_send_tx,
            pkt_receive_rx,
        )
    }
}

pub enum Address {
    HostnameAddress(String, u16),
    SocketAddress(SocketAddr),
}
