use super::{
    incoming::{self, Sender as IncomingSender},
    request::Wait as WaitRequest,
    stream::{BiStream, IncomingUniStreams, RecvStream, Register as StreamRegister, SendStream},
    Address, ServerAddr, UdpRelayMode,
};
use bytes::Bytes;
use parking_lot::Mutex;
use quinn::{ClientConfig, Connection as QuinnConnection, Datagrams, Endpoint, NewConnection};
use std::{
    collections::HashMap,
    future::Future,
    io::{Error, ErrorKind, Result},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{
    net,
    sync::{mpsc::Sender as MpscSender, Mutex as AsyncMutex, OwnedMutexGuard},
    time,
};
use tuic_protocol::Command;

pub async fn manage_connection(
    config: ConnectionConfig,
    conn: Arc<AsyncMutex<Option<Connection>>>,
    lock: OwnedMutexGuard<Option<Connection>>,
    mut next_incoming_tx: UdpRelayMode<
        IncomingSender<Datagrams>,
        IncomingSender<IncomingUniStreams>,
    >,
    wait_req: WaitRequest,
) {
    let mut lock = Some(lock);

    loop {
        // establish a new connection
        let new_conn = loop {
            // start the procedure only if there is a request waiting
            wait_req.clone().await;

            // try to establish a new connection
            let (new_conn, dg, uni) = match Connection::connect(&config).await {
                Ok(conn) => conn,
                Err(err) => {
                    log::error!("[relay] [connection] {err}");

                    // sleep 1 second to avoid drawing too much CPU
                    time::sleep(Duration::from_secs(1)).await;

                    continue;
                }
            };

            // renew the connection mutex
            // safety: the mutex must be locked before, so this container must have a lock guard inside
            let mut lock = lock.take().unwrap();
            *lock.deref_mut() = Some(new_conn.clone());

            // send the incoming streams to `incoming::listen_incoming`
            match next_incoming_tx {
                UdpRelayMode::Native(incoming_tx) => {
                    let (tx, rx) = incoming::channel::<Datagrams>();
                    let _ = incoming_tx.send(new_conn.clone(), dg, rx);
                    next_incoming_tx = UdpRelayMode::Native(tx);
                }
                UdpRelayMode::Quic(incoming_tx) => {
                    let (tx, rx) = incoming::channel::<IncomingUniStreams>();
                    let _ = incoming_tx.send(new_conn.clone(), uni, rx);
                    next_incoming_tx = UdpRelayMode::Quic(tx);
                }
            }

            new_conn.update_max_udp_relay_packet_size();

            // connection established, drop the lock implicitly
            break new_conn;
        };

        log::debug!("[relay] [connection] [establish]");

        // wait for the connection to be closed, lock the mutex
        new_conn.wait_close().await;

        log::debug!("[relay] [connection] [disconnect]");
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
    default_max_udp_relay_packet_size: usize,
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
        let mut last_err = None;

        for addr in addrs {
            match Self::connect_addr(config, addr, name).await {
                Ok(new_conn) => {
                    conn = Some(new_conn);
                    break;
                }
                Err(err) => last_err = Some(err),
            }
        }

        conn.ok_or_else(|| {
            last_err
                .unwrap_or_else(|| Error::new(ErrorKind::Other, "Unable to connect to the server"))
        })
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
                Err(conn) => {
                    log::warn!("[relay] [connection] Unable to convert the connection into 0-RTT");
                    conn.await?
                }
            }
        } else {
            conn.await?
        };

        let conn = Self::new(connection, config).await;
        let uni_streams = IncomingUniStreams::new(uni_streams, conn.stream_reg.get_registry());

        Ok((conn, datagrams, uni_streams))
    }

    async fn new(conn: QuinnConnection, config: &ConnectionConfig) -> Self {
        let conn = Self {
            controller: conn,
            udp_sessions: Arc::new(UdpSessionMap::new()),
            stream_reg: Arc::new(StreamRegister::new()),
            udp_relay_mode: config.udp_relay_mode,
            is_closed: IsClosed::new(),
            default_max_udp_relay_packet_size: config.max_udp_relay_packet_size,
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
            send.finish().await?;
            Ok(())
        }

        match send_token(&self, token_digest).await {
            Ok(()) => log::debug!("[relay] [connection] [authentication]"),
            Err(err) => log::warn!("[relay] [connection] [authentication] {err}"),
        }
    }

    async fn heartbeat(self, heartbeat_interval: u64) {
        async fn send_heartbeat(conn: &Connection) -> Result<()> {
            let mut send = conn.get_send_stream().await?;
            let cmd = Command::new_heartbeat();
            cmd.write_to(&mut send).await?;
            send.finish().await?;
            Ok(())
        }

        let mut interval = time::interval(Duration::from_millis(heartbeat_interval));

        while tokio::select! {
            () = self.wait_close() => false,
            _ = interval.tick() => true,
        } {
            if !self.no_active_stream() || !self.no_active_udp_session() {
                match send_heartbeat(&self).await {
                    Ok(()) => log::debug!("[relay] [connection] [heartbeat]"),
                    Err(err) => log::warn!("[relay] [connection] [heartbeat] {err}"),
                }
            }
        }
    }

    pub async fn get_send_stream(&self) -> Result<SendStream> {
        let send = self.controller.open_uni().await?;
        let reg = (*self.stream_reg).clone(); // clone inner, not itself
        Ok(SendStream::new(send, reg))
    }

    pub async fn get_bi_stream(&self) -> Result<BiStream> {
        let (send, recv) = self.controller.open_bi().await?;
        let reg = (*self.stream_reg).clone(); // clone inner, not itself

        Ok(BiStream::new(
            SendStream::new(send, reg.clone()),
            RecvStream::new(recv, reg),
        ))
    }

    pub fn send_datagram(&self, data: Bytes) -> Result<()> {
        self.controller
            .send_datagram(data)
            .map_err(|err| Error::new(ErrorKind::Other, err))
    }

    pub fn udp_sessions(&self) -> &UdpSessionMap {
        self.udp_sessions.deref()
    }

    pub fn udp_relay_mode(&self) -> UdpRelayMode<(), ()> {
        self.udp_relay_mode
    }

    pub fn update_max_udp_relay_packet_size(&self) {
        let size = match self.udp_relay_mode {
            UdpRelayMode::Native(()) => match self.controller.max_datagram_size() {
                Some(size) => size,
                None => {
                    log::warn!("[relay] [connection] Failed to detect the max datagram size");
                    self.default_max_udp_relay_packet_size
                }
            },
            UdpRelayMode::Quic(()) => self.default_max_udp_relay_packet_size,
        };

        super::MAX_UDP_RELAY_PACKET_SIZE.store(size, Ordering::Release);
    }

    fn no_active_stream(&self) -> bool {
        self.stream_reg.count() == 1
    }

    fn no_active_udp_session(&self) -> bool {
        self.udp_sessions.is_empty()
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
    max_udp_relay_packet_size: usize,
}

impl ConnectionConfig {
    pub fn new(
        quinn_config: ClientConfig,
        server_addr: ServerAddr,
        token_digest: [u8; 32],
        udp_relay_mode: UdpRelayMode<(), ()>,
        heartbeat_interval: u64,
        reduce_rtt: bool,
        max_udp_relay_packet_size: usize,
    ) -> Self {
        Self {
            quinn_config,
            server_addr,
            token_digest,
            udp_relay_mode,
            heartbeat_interval,
            reduce_rtt,
            max_udp_relay_packet_size,
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

    fn is_empty(&self) -> bool {
        self.0.lock().is_empty()
    }
}

#[derive(Clone)]
struct IsClosed(Arc<IsClosedInner>);

struct IsClosedInner {
    is_closed: AtomicBool,
    waker: Mutex<Vec<Waker>>,
}

impl IsClosed {
    fn new() -> Self {
        Self(Arc::new(IsClosedInner {
            is_closed: AtomicBool::new(false),
            // Needs at least 2 slots for `manage_connection()` and `heartbeat()`
            waker: Mutex::new(Vec::with_capacity(2)),
        }))
    }

    fn set(&self) {
        self.0.is_closed.store(true, Ordering::Release);

        for waker in self.0.waker.lock().drain(..) {
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
            self.0.waker.lock().push(cx.waker().clone());
            Poll::Pending
        }
    }
}
