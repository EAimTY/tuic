use super::{
    incoming::{self, Receiver as IncomingReceiver, Sender as IncomingSender},
    request::Wait as WaitRequest,
    stream::{BiStream, Register as StreamRegister, SendStream},
    Address, ServerAddr, UdpRelayMode,
};
use bytes::Bytes;
use parking_lot::Mutex;
use quinn::{
    ClientConfig, Connection as QuinnConnection, Datagrams, Endpoint, IncomingUniStreams,
    NewConnection,
};
use std::{
    collections::HashMap,
    future::Future,
    io::{Error, ErrorKind, Result},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
use tokio::{
    io::AsyncWriteExt,
    net,
    sync::{
        mpsc::Sender as MpscSender,
        oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
        Mutex as AsyncMutex, OwnedMutexGuard,
    },
};
use tuic_protocol::Command;

pub async fn manage_connection(
    config: ConnectionConfig,
    conn: Arc<AsyncMutex<Connection>>,
    lock: OwnedMutexGuard<Connection>,
    mut next_incoming_tx: UdpRelayMode<
        IncomingSender<Datagrams>,
        IncomingSender<IncomingUniStreams>,
    >,
    wait_req: WaitRequest,
) {
    let mut lock = Some(lock);

    loop {
        // establish a new connection
        let mut new_conn;

        loop {
            // start the procedure only if there is a request waiting
            wait_req.clone().await;

            // try to establish a new connection
            let (dg, uni);
            (new_conn, dg, uni) = match Connection::connect(&config).await {
                Ok(conn) => conn,
                Err(err) => {
                    log::error!("{err}");
                    continue;
                }
            };

            // renew the connection mutex
            let mut lock = lock.take().unwrap(); // safety: the mutex must be locked before
            *lock.deref_mut() = new_conn.clone();

            // send the incoming streams to `incoming::listen_incoming`
            match next_incoming_tx {
                UdpRelayMode::Native(incoming_tx) => {
                    let (tx, rx) = incoming::channel::<Datagrams>();
                    incoming_tx.send(new_conn.clone(), dg, rx);
                    next_incoming_tx = UdpRelayMode::Native(tx);
                }
                UdpRelayMode::Quic(incoming_tx) => {
                    let (tx, rx) = incoming::channel::<IncomingUniStreams>();
                    incoming_tx.send(new_conn.clone(), uni, rx);
                    next_incoming_tx = UdpRelayMode::Quic(tx);
                }
            }

            // connection established, drop the lock implicitly
            break;
        }

        // wait for the connection to be closed, lock the mutex
        new_conn.wait_close().await;
        lock = Some(conn.clone().lock_owned().await);
    }
}

#[derive(Clone)]
pub struct Connection {
    controller: QuinnConnection,
    udp_sessions: Arc<UdpSessionMap>,
    stream_reg: Arc<StreamRegister>,
    udp_relay_mode: UdpRelayMode<(), ()>,
    is_closed: IsClosed,
}

impl Connection {
    async fn connect(config: &ConnectionConfig) -> Result<(Self, Datagrams, IncomingUniStreams)> {
        let (addrs, name) = match &config.server_addr {
            ServerAddr::SocketAddr { addr, name } => Ok((vec![*addr], name)),
            ServerAddr::DomainAddr { domain, port } => net::lookup_host((domain.as_str(), *port))
                .await
                .map(|res| (res.collect(), domain)),
        }?;

        let mut conn = None;

        for addr in addrs {
            match Self::connect_addr(config, addr, name).await {
                Ok(new_conn) => {
                    conn = Some(new_conn);
                    break;
                }
                Err(err) => eprintln!("{err}"),
            }
        }

        conn.ok_or(Error::new(ErrorKind::Other, "err"))
    }

    async fn connect_addr(
        config: &ConnectionConfig,
        addr: SocketAddr,
        name: &str,
    ) -> Result<(Self, Datagrams, IncomingUniStreams)> {
        let bind_addr = match addr {
            SocketAddr::V4(_) => SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
            SocketAddr::V6(_) => SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
        };

        let conn = Endpoint::client(bind_addr)?
            .connect_with(config.quinn_config.clone(), addr, name)
            .map_err(|err| Error::new(ErrorKind::Other, err))?;

        let NewConnection {
            connection,
            datagrams,
            uni_streams,
            ..
        } = if config.reduce_rtt {
            match conn.into_0rtt() {
                Ok((conn, _)) => conn,
                Err(conn) => conn.await?,
            }
        } else {
            conn.await?
        };

        Ok((Self::new(connection, config).await, datagrams, uni_streams))
    }

    async fn new(conn: QuinnConnection, config: &ConnectionConfig) -> Self {
        let conn = Self {
            controller: conn,
            udp_sessions: Arc::new(UdpSessionMap::new()),
            stream_reg: Arc::new(StreamRegister::new()),
            udp_relay_mode: config.udp_relay_mode,
            is_closed: IsClosed::new(),
        };

        // send auth
        tokio::spawn(Self::send_authentication(conn.clone(), config.token_digest));

        // heartbeat
        tokio::spawn(Self::heartbeat(conn.clone(), config.heartbeat_interval));

        conn
    }

    async fn send_authentication(self, token_digest: [u8; 32]) {
        async fn send_token(conn: &Connection, token_digest: [u8; 32]) -> Result<()> {
            let mut send = conn.get_send_stream().await?;
            let cmd = Command::new_authenticate(token_digest);
            cmd.write_to(&mut send).await?;
            let _ = send.shutdown().await;
            Ok(())
        }

        match send_token(&self, token_digest).await {
            Ok(()) => log::debug!("[relay] [connection] [authentication]"),
            Err(err) => log::error!("[relay] [connection] [authentication] {err}"),
        }
    }

    async fn heartbeat(self, heartbeat_interval: u64) {
        todo!();
    }

    pub async fn get_send_stream(&self) -> Result<SendStream> {
        let send = self.controller.open_uni().await?;
        let reg = (*self.stream_reg).clone(); // clone inner, not itself
        Ok(SendStream::new(send, reg))
    }

    pub async fn get_bi_stream(&self) -> Result<BiStream> {
        let (send, recv) = self.controller.open_bi().await?;
        let reg = (*self.stream_reg).clone(); // clone inner, not itself
        Ok(BiStream::new(send, recv, reg))
    }

    pub fn udp_sessions(&self) -> &UdpSessionMap {
        self.udp_sessions.deref()
    }

    pub fn udp_relay_mode(&self) -> UdpRelayMode<(), ()> {
        self.udp_relay_mode
    }

    pub fn is_closed(&self) -> bool {
        self.is_closed.get()
    }

    pub fn set_closed(&self) {
        self.is_closed.set()
    }

    fn wait_close(&self) -> IsClosed {
        self.is_closed.clone()
    }
}

pub struct ConnectionConfig {
    quinn_config: ClientConfig,
    server_addr: ServerAddr,
    token_digest: [u8; 32],
    udp_relay_mode: UdpRelayMode<(), ()>,
    heartbeat_interval: u64,
    reduce_rtt: bool,
}

impl ConnectionConfig {
    pub fn new(
        quinn_config: ClientConfig,
        server_addr: ServerAddr,
        token_digest: [u8; 32],
        udp_relay_mode: UdpRelayMode<(), ()>,
        heartbeat_interval: u64,
        reduce_rtt: bool,
    ) -> Self {
        Self {
            quinn_config,
            server_addr,
            token_digest,
            udp_relay_mode,
            heartbeat_interval,
            reduce_rtt,
        }
    }
}

pub struct UdpSessionMap(Mutex<HashMap<u32, MpscSender<(Bytes, Address)>>>);

impl UdpSessionMap {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    pub fn insert(
        &self,
        id: u32,
        tx: MpscSender<(Bytes, Address)>,
    ) -> Option<MpscSender<(Bytes, Address)>> {
        self.0.lock().insert(id, tx)
    }

    pub fn get(&self, id: &u32) -> Option<MpscSender<(Bytes, Address)>> {
        self.0.lock().get(id).cloned()
    }

    pub fn remove(&self, id: &u32) -> Option<MpscSender<(Bytes, Address)>> {
        self.0.lock().remove(id)
    }
}

#[derive(Clone)]
struct IsClosed(Arc<IsClosedInner>);

struct IsClosedInner {
    is_closed: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl IsClosed {
    fn new() -> Self {
        Self(Arc::new(IsClosedInner {
            is_closed: AtomicBool::new(false),
            waker: Mutex::new(None),
        }))
    }

    fn get(&self) -> bool {
        self.0.is_closed.load(Ordering::Acquire)
    }

    fn set(&self) {
        self.0.is_closed.store(true, Ordering::Release);

        if let Some(waker) = self.0.waker.lock().take() {
            waker.wake();
        }
    }
}

impl Future for IsClosed {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0.is_closed.load(Ordering::Acquire) {
            Poll::Ready(())
        } else {
            *self.0.waker.lock() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
