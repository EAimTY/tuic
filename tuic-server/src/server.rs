use crate::{
    config::Config,
    utils::{self, CongestionControl, UdpRelayMode},
    Error,
};
use bytes::Bytes;
use crossbeam_utils::atomic::AtomicCell;
use parking_lot::Mutex;
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    Connecting, Connection as QuinnConnection, ConnectionError, Endpoint, EndpointConfig,
    IdleTimeout, RecvStream, SendStream, ServerConfig, TokioRuntime, TransportConfig, VarInt,
};
use register_count::{Counter, Register};
use rustls::{version, ServerConfig as RustlsServerConfig};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    collections::{hash_map::Entry, HashMap},
    future::Future,
    io::{Error as IoError, ErrorKind},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket as StdUdpSocket},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{
    io::{self, AsyncWriteExt},
    net::{self, TcpStream, UdpSocket},
    sync::{
        oneshot::{self, Receiver, Sender},
        Mutex as AsyncMutex,
    },
    time,
};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tuic::Address;
use tuic_quinn::{side, Authenticate, Connect, Connection as Model, Packet, Task};
use uuid::Uuid;

const ERROR_CODE: VarInt = VarInt::from_u32(0);
const DEFAULT_CONCURRENT_STREAMS: usize = 32;

pub struct Server {
    ep: Endpoint,
    users: Arc<HashMap<Uuid, Vec<u8>>>,
    udp_relay_ipv6: bool,
    zero_rtt_handshake: bool,
    auth_timeout: Duration,
    task_negotiation_timeout: Duration,
    max_external_pkt_size: usize,
    gc_interval: Duration,
    gc_lifetime: Duration,
}

impl Server {
    pub fn init(cfg: Config) -> Result<Self, Error> {
        let certs = utils::load_certs(cfg.certificate)?;
        let priv_key = utils::load_priv_key(cfg.private_key)?;

        let mut crypto = RustlsServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(certs, priv_key)?;

        crypto.alpn_protocols = cfg.alpn.into_iter().map(|alpn| alpn.into_bytes()).collect();
        crypto.max_early_data_size = u32::MAX;
        crypto.send_half_rtt_data = cfg.zero_rtt_handshake;

        let mut config = ServerConfig::with_crypto(Arc::new(crypto));
        let mut tp_cfg = TransportConfig::default();

        tp_cfg
            .max_concurrent_bidi_streams(VarInt::from(DEFAULT_CONCURRENT_STREAMS as u32))
            .max_concurrent_uni_streams(VarInt::from(DEFAULT_CONCURRENT_STREAMS as u32))
            .send_window(cfg.send_window)
            .stream_receive_window(VarInt::from_u32(cfg.receive_window))
            .max_idle_timeout(Some(
                IdleTimeout::try_from(cfg.max_idle_time).map_err(|_| Error::InvalidMaxIdleTime)?,
            ));

        match cfg.congestion_control {
            CongestionControl::Cubic => {
                tp_cfg.congestion_controller_factory(Arc::new(CubicConfig::default()))
            }
            CongestionControl::NewReno => {
                tp_cfg.congestion_controller_factory(Arc::new(NewRenoConfig::default()))
            }
            CongestionControl::Bbr => {
                tp_cfg.congestion_controller_factory(Arc::new(BbrConfig::default()))
            }
        };

        config.transport_config(Arc::new(tp_cfg));

        let socket = {
            let domain = match cfg.server {
                SocketAddr::V4(_) => Domain::IPV4,
                SocketAddr::V6(_) => Domain::IPV6,
            };

            let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
                .map_err(|err| Error::Socket("failed to create endpoint UDP socket", err))?;

            if let Some(dual_stack) = cfg.dual_stack {
                socket.set_only_v6(!dual_stack).map_err(|err| {
                    Error::Socket("endpoint dual-stack socket setting error", err)
                })?;
            }

            socket
                .bind(&SockAddr::from(cfg.server))
                .map_err(|err| Error::Socket("failed to bind endpoint UDP socket", err))?;

            StdUdpSocket::from(socket)
        };

        let ep = Endpoint::new(
            EndpointConfig::default(),
            Some(config),
            socket,
            Arc::new(TokioRuntime),
        )?;

        let users = cfg
            .users
            .into_iter()
            .map(|(uuid, password)| (uuid, password.into_bytes()))
            .collect();

        Ok(Self {
            ep,
            users: Arc::new(users),
            udp_relay_ipv6: cfg.udp_relay_ipv6,
            zero_rtt_handshake: cfg.zero_rtt_handshake,
            auth_timeout: cfg.auth_timeout,
            task_negotiation_timeout: cfg.task_negotiation_timeout,
            max_external_pkt_size: cfg.max_external_packet_size,
            gc_interval: cfg.gc_interval,
            gc_lifetime: cfg.gc_lifetime,
        })
    }

    pub async fn start(&self) {
        log::warn!(
            "server started, listening on {}",
            self.ep.local_addr().unwrap()
        );

        loop {
            let Some(conn) = self.ep.accept().await else {
                return;
            };

            tokio::spawn(Connection::handle(
                conn,
                self.users.clone(),
                self.udp_relay_ipv6,
                self.zero_rtt_handshake,
                self.auth_timeout,
                self.task_negotiation_timeout,
                self.max_external_pkt_size,
                self.gc_interval,
                self.gc_lifetime,
            ));
        }
    }
}

#[derive(Clone)]
struct Connection {
    inner: QuinnConnection,
    model: Model<side::Server>,
    users: Arc<HashMap<Uuid, Vec<u8>>>,
    udp_relay_ipv6: bool,
    auth: Authenticated,
    task_negotiation_timeout: Duration,
    udp_sessions: Arc<AsyncMutex<HashMap<u16, UdpSession>>>,
    udp_relay_mode: Arc<AtomicCell<Option<UdpRelayMode>>>,
    max_external_pkt_size: usize,
    remote_uni_stream_cnt: Counter,
    remote_bi_stream_cnt: Counter,
    max_concurrent_uni_streams: Arc<AtomicUsize>,
    max_concurrent_bi_streams: Arc<AtomicUsize>,
}

#[allow(clippy::too_many_arguments)]
impl Connection {
    async fn handle(
        conn: Connecting,
        users: Arc<HashMap<Uuid, Vec<u8>>>,
        udp_relay_ipv6: bool,
        zero_rtt_handshake: bool,
        auth_timeout: Duration,
        task_negotiation_timeout: Duration,
        max_external_pkt_size: usize,
        gc_interval: Duration,
        gc_lifetime: Duration,
    ) {
        let addr = conn.remote_address();

        let init = async {
            let conn = if zero_rtt_handshake {
                match conn.into_0rtt() {
                    Ok((conn, _)) => conn,
                    Err(conn) => conn.await?,
                }
            } else {
                conn.await?
            };

            Ok::<_, Error>(Self::new(
                conn,
                users,
                udp_relay_ipv6,
                task_negotiation_timeout,
                max_external_pkt_size,
            ))
        };

        match init.await {
            Ok(conn) => {
                log::info!("[{addr}] connection established");

                tokio::spawn(conn.clone().timeout_authenticate(auth_timeout));
                tokio::spawn(conn.clone().collect_garbage(gc_interval, gc_lifetime));

                loop {
                    if conn.is_closed() {
                        break;
                    }

                    let handle_incoming = async {
                        tokio::select! {
                            res = conn.inner.accept_uni() =>
                                tokio::spawn(conn.clone().handle_uni_stream(res?, conn.remote_uni_stream_cnt.reg())),
                            res = conn.inner.accept_bi() =>
                                tokio::spawn(conn.clone().handle_bi_stream(res?, conn.remote_bi_stream_cnt.reg())),
                            res = conn.inner.read_datagram() =>
                                tokio::spawn(conn.clone().handle_datagram(res?)),
                        };

                        Ok::<_, Error>(())
                    };

                    match handle_incoming.await {
                        Ok(()) => {}
                        Err(err) if err.is_locally_closed() => {}
                        Err(err) if err.is_timeout_closed() => {
                            log::debug!("[{addr}] connection timeout")
                        }
                        Err(err) => log::warn!("[{addr}] connection error: {err}"),
                    }
                }
            }
            Err(err) if err.is_locally_closed() || err.is_timeout_closed() => unreachable!(),
            Err(err) => log::warn!("[{addr}] connection establishing error: {err}"),
        }
    }

    fn new(
        conn: QuinnConnection,
        users: Arc<HashMap<Uuid, Vec<u8>>>,
        udp_relay_ipv6: bool,
        task_negotiation_timeout: Duration,
        max_external_pkt_size: usize,
    ) -> Self {
        Self {
            inner: conn.clone(),
            model: Model::<side::Server>::new(conn),
            users,
            udp_relay_ipv6,
            auth: Authenticated::new(),
            task_negotiation_timeout,
            udp_sessions: Arc::new(AsyncMutex::new(HashMap::new())),
            udp_relay_mode: Arc::new(AtomicCell::new(None)),
            max_external_pkt_size,
            remote_uni_stream_cnt: Counter::new(),
            remote_bi_stream_cnt: Counter::new(),
            max_concurrent_uni_streams: Arc::new(AtomicUsize::new(DEFAULT_CONCURRENT_STREAMS)),
            max_concurrent_bi_streams: Arc::new(AtomicUsize::new(DEFAULT_CONCURRENT_STREAMS)),
        }
    }

    fn authenticate(&self, auth: &Authenticate) -> Result<(), Error> {
        if self.auth.get().is_some() {
            Err(Error::DuplicatedAuth)
        } else if self
            .users
            .get(&auth.uuid())
            .map_or(false, |password| auth.validate(password))
        {
            self.auth.set(auth.uuid());
            Ok(())
        } else {
            Err(Error::AuthFailed(auth.uuid()))
        }
    }

    async fn timeout_authenticate(self, timeout: Duration) {
        time::sleep(timeout).await;

        if self.auth.get().is_none() {
            let addr = self.inner.remote_address();
            log::warn!("[{addr}] [authenticate] timeout");
            self.close();
        }
    }

    async fn handle_uni_stream(self, recv: RecvStream, _reg: Register) {
        let addr = self.inner.remote_address();
        log::debug!("[{addr}] incoming unidirectional stream");

        let max = self.max_concurrent_uni_streams.load(Ordering::Relaxed);

        if self.remote_uni_stream_cnt.count() == max {
            self.max_concurrent_uni_streams
                .store(max * 2, Ordering::Relaxed);

            self.inner
                .set_max_concurrent_uni_streams(VarInt::from((max * 2) as u32));
        }

        let pre_process = async {
            let task = time::timeout(
                self.task_negotiation_timeout,
                self.model.accept_uni_stream(recv),
            )
            .await
            .map_err(|_| Error::TaskNegotiationTimeout)??;

            if let Task::Authenticate(auth) = &task {
                self.authenticate(auth)?;
            }

            tokio::select! {
                () = self.auth.clone() => {}
                err = self.inner.closed() => return Err(Error::Connection(err)),
            };

            let same_pkt_src = matches!(task, Task::Packet(_))
                && matches!(self.get_udp_relay_mode(), Some(UdpRelayMode::Native));
            if same_pkt_src {
                return Err(Error::UnexpectedPacketSource);
            }

            Ok(task)
        };

        match pre_process.await {
            Ok(Task::Authenticate(auth)) => self.handle_authenticate(auth).await,
            Ok(Task::Packet(pkt)) => self.handle_packet(pkt, UdpRelayMode::Quic).await,
            Ok(Task::Dissociate(assoc_id)) => self.handle_dissociate(assoc_id).await,
            Ok(_) => unreachable!(), // already filtered in `tuic_quinn`
            Err(err) => {
                log::warn!("[{addr}] handle unidirection stream error: {err}");
                self.close();
            }
        }
    }

    async fn handle_bi_stream(self, (send, recv): (SendStream, RecvStream), _reg: Register) {
        let addr = self.inner.remote_address();
        log::debug!("[{addr}] incoming bidirectional stream");

        let max = self.max_concurrent_bi_streams.load(Ordering::Relaxed);

        if self.remote_bi_stream_cnt.count() == max {
            self.max_concurrent_bi_streams
                .store(max * 2, Ordering::Relaxed);

            self.inner
                .set_max_concurrent_bi_streams(VarInt::from((max * 2) as u32));
        }

        let pre_process = async {
            let task = time::timeout(
                self.task_negotiation_timeout,
                self.model.accept_bi_stream(send, recv),
            )
            .await
            .map_err(|_| Error::TaskNegotiationTimeout)??;

            tokio::select! {
                () = self.auth.clone() => {}
                err = self.inner.closed() => return Err(Error::Connection(err)),
            };

            Ok(task)
        };

        match pre_process.await {
            Ok(Task::Connect(conn)) => self.handle_connect(conn).await,
            Ok(_) => unreachable!(), // already filtered in `tuic_quinn`
            Err(err) => {
                log::warn!("[{addr}] handle bidirection stream error: {err}");
                self.close();
            }
        }
    }

    async fn handle_datagram(self, dg: Bytes) {
        let addr = self.inner.remote_address();
        log::debug!("[{addr}] incoming datagram");

        let pre_process = async {
            let task = self.model.accept_datagram(dg)?;

            tokio::select! {
                () = self.auth.clone() => {}
                err = self.inner.closed() => return Err(Error::Connection(err)),
            };

            let same_pkt_src = matches!(task, Task::Packet(_))
                && matches!(self.get_udp_relay_mode(), Some(UdpRelayMode::Quic));
            if same_pkt_src {
                return Err(Error::UnexpectedPacketSource);
            }

            Ok(task)
        };

        match pre_process.await {
            Ok(Task::Packet(pkt)) => self.handle_packet(pkt, UdpRelayMode::Native).await,
            Ok(Task::Heartbeat) => self.handle_heartbeat().await,
            Ok(_) => unreachable!(),
            Err(err) => {
                log::warn!("[{addr}] handle datagram error: {err}");
                self.close();
            }
        }
    }

    async fn handle_authenticate(&self, auth: Authenticate) {
        log::info!(
            "[{addr}] [{uuid}] [authenticate] authenticated as {auth_uuid}",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
            auth_uuid = auth.uuid(),
        );
    }

    async fn handle_connect(&self, conn: Connect) {
        let target_addr = conn.addr().to_string();

        log::info!(
            "[{addr}] [{uuid}] [connect] {target_addr}",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
        );

        let process = async {
            let mut stream = None;
            let mut last_err = None;

            match resolve_dns(conn.addr()).await {
                Ok(addrs) => {
                    for addr in addrs {
                        match TcpStream::connect(addr).await {
                            Ok(s) => {
                                stream = Some(s);
                                break;
                            }
                            Err(err) => last_err = Some(err),
                        }
                    }
                }
                Err(err) => last_err = Some(err),
            }

            if let Some(mut stream) = stream {
                let mut conn = conn.compat();
                let res = io::copy_bidirectional(&mut conn, &mut stream).await;
                let _ = conn.get_mut().reset(ERROR_CODE);
                let _ = stream.shutdown().await;
                res?;
                Ok::<_, Error>(())
            } else {
                let _ = conn.compat().shutdown().await;
                Err(last_err
                    .unwrap_or_else(|| IoError::new(ErrorKind::NotFound, "no address resolved")))?
            }
        };

        match process.await {
            Ok(()) => {}
            Err(err) => log::warn!(
                "[{addr}] [{uuid}] [connect] relaying connection to {target_addr} error: {err}",
                addr = self.inner.remote_address(),
                uuid = self.auth.get().unwrap(),
            ),
        }
    }

    async fn handle_packet(&self, pkt: Packet, mode: UdpRelayMode) {
        let assoc_id = pkt.assoc_id();
        let pkt_id = pkt.pkt_id();
        let frag_id = pkt.frag_id();
        let frag_total = pkt.frag_total();

        log::info!(
            "[{addr}] [{uuid}] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] {frag_id}/{frag_total}",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
        );

        self.set_udp_relay_mode(mode);

        let process = async {
            let Some((pkt, addr, assoc_id)) = pkt.accept().await? else {
                return Ok(());
            };

            let (socket_v4, socket_v6) = match self.udp_sessions.lock().await.entry(assoc_id) {
                Entry::Occupied(mut entry) => {
                    let session = entry.get_mut();
                    (session.socket_v4.clone(), session.socket_v6.clone())
                }
                Entry::Vacant(entry) => {
                    let session = entry.insert(
                        UdpSession::new(assoc_id, self.clone(), self.udp_relay_ipv6).await?,
                    );

                    (session.socket_v4.clone(), session.socket_v6.clone())
                }
            };

            let Some(socket_addr) = resolve_dns(&addr).await?.next() else {
                return Err(Error::from(IoError::new(ErrorKind::NotFound, "no address resolved")));
            };

            let socket = match socket_addr {
                SocketAddr::V4(_) => socket_v4,
                SocketAddr::V6(_) => {
                    socket_v6.ok_or_else(|| Error::UdpRelayIpv6Disabled(addr, socket_addr))?
                }
            };

            socket.send_to(&pkt, socket_addr).await?;

            Ok(())
        };

        match process.await {
            Ok(()) => {}
            Err(err) => log::warn!(
                "[{addr}] [{uuid}] [packet] [{assoc_id:#06x}] [from-{mode}] [{pkt_id:#06x}] error handling fragment {frag_id}/{frag_total}: {err}",
                addr = self.inner.remote_address(),
                uuid = self.auth.get().unwrap(),
            ),
        }
    }

    async fn handle_dissociate(&self, assoc_id: u16) {
        log::info!(
            "[{addr}] [{uuid}] [dissociate] [{assoc_id:#06x}]",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
        );

        self.udp_sessions.lock().await.remove(&assoc_id);
    }

    async fn handle_heartbeat(&self) {
        log::info!(
            "[{addr}] [{uuid}] [heartbeat]",
            addr = self.inner.remote_address(),
            uuid = self.auth.get().unwrap(),
        );
    }

    async fn collect_garbage(self, gc_interval: Duration, gc_lifetime: Duration) {
        loop {
            time::sleep(gc_interval).await;

            if self.is_closed() {
                break;
            }

            self.model.collect_garbage(gc_lifetime);
        }
    }

    fn set_udp_relay_mode(&self, mode: UdpRelayMode) {
        self.udp_relay_mode.store(Some(mode));
    }

    fn get_udp_relay_mode(&self) -> Option<UdpRelayMode> {
        self.udp_relay_mode.load()
    }

    fn is_closed(&self) -> bool {
        self.inner.close_reason().is_some()
    }

    fn close(&self) {
        self.inner.close(ERROR_CODE, &[]);
    }
}

async fn resolve_dns(addr: &Address) -> Result<impl Iterator<Item = SocketAddr>, IoError> {
    match addr {
        Address::None => Err(IoError::new(ErrorKind::InvalidInput, "empty address")),
        Address::DomainAddress(domain, port) => Ok(net::lookup_host((domain.as_str(), *port))
            .await?
            .collect::<Vec<_>>()
            .into_iter()),
        Address::SocketAddress(addr) => Ok(vec![*addr].into_iter()),
    }
}

struct UdpSession {
    socket_v4: Arc<UdpSocket>,
    socket_v6: Option<Arc<UdpSocket>>,
    cancel: Option<Sender<()>>,
}

impl UdpSession {
    async fn new(assoc_id: u16, conn: Connection, udp_relay_ipv6: bool) -> Result<Self, Error> {
        let socket_v4 = Arc::new(
            UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))
                .await
                .map_err(|err| Error::Socket("failed to create UDP associate IPv4 socket", err))?,
        );
        let socket_v6 = if udp_relay_ipv6 {
            let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))
                .map_err(|err| Error::Socket("failed to create UDP associate IPv6 socket", err))?;

            socket.set_nonblocking(true).map_err(|err| {
                Error::Socket(
                    "failed setting UDP associate IPv6 socket as non-blocking",
                    err,
                )
            })?;

            socket.set_only_v6(true).map_err(|err| {
                Error::Socket("failed setting UDP associate IPv6 socket as IPv6-only", err)
            })?;

            socket
                .bind(&SockAddr::from(SocketAddr::from((
                    Ipv6Addr::UNSPECIFIED,
                    0,
                ))))
                .map_err(|err| Error::Socket("failed to bind UDP associate IPv6 socket", err))?;

            Some(Arc::new(UdpSocket::from_std(StdUdpSocket::from(socket))?))
        } else {
            None
        };

        let (tx, rx) = oneshot::channel();

        tokio::spawn(Self::listen_incoming(
            assoc_id,
            conn,
            socket_v4.clone(),
            socket_v6.clone(),
            rx,
        ));

        Ok(Self {
            socket_v4,
            socket_v6,
            cancel: Some(tx),
        })
    }

    async fn listen_incoming(
        assoc_id: u16,
        conn: Connection,
        socket_v4: Arc<UdpSocket>,
        socket_v6: Option<Arc<UdpSocket>>,
        cancel: Receiver<()>,
    ) {
        async fn send_pkt(conn: Connection, pkt: Bytes, target_addr: SocketAddr, assoc_id: u16) {
            let addr = conn.inner.remote_address();
            let target_addr_tuic = Address::SocketAddress(target_addr);

            let res = match conn.get_udp_relay_mode() {
                Some(UdpRelayMode::Native) => {
                    log::info!("[{addr}] [packet-to-native] [{assoc_id}] [{target_addr_tuic}]");
                    conn.model.packet_native(pkt, target_addr_tuic, assoc_id)
                }
                Some(UdpRelayMode::Quic) => {
                    log::info!("[{addr}] [packet-to-quic] [{assoc_id}] [{target_addr_tuic}]");
                    conn.model
                        .packet_quic(pkt, target_addr_tuic, assoc_id)
                        .await
                }
                None => unreachable!(),
            };

            if let Err(err) = res {
                let target_addr_tuic = Address::SocketAddress(target_addr);
                log::warn!("[{addr}] [packet-to-quic] [{assoc_id}] [{target_addr_tuic}] {err}");
            }
        }

        let addr = conn.inner.remote_address();

        tokio::select! {
            _ = cancel => {}
            () = async {
                loop {
                    match Self::accept(
                        &socket_v4,
                        socket_v6.as_deref(),
                        conn.max_external_pkt_size,
                    ).await {
                        Ok((pkt, target_addr)) => {
                            tokio::spawn(send_pkt(conn.clone(), pkt, target_addr, assoc_id));
                        }
                        Err(err) => log::warn!("[{addr}] [packet-to-*] [{assoc_id}] {err}"),
                    }
                }
            } => unreachable!(),
        }
    }

    async fn accept(
        socket_v4: &UdpSocket,
        socket_v6: Option<&UdpSocket>,
        max_pkt_size: usize,
    ) -> Result<(Bytes, SocketAddr), IoError> {
        async fn read_pkt(
            socket: &UdpSocket,
            max_pkt_size: usize,
        ) -> Result<(Bytes, SocketAddr), IoError> {
            let mut buf = vec![0u8; max_pkt_size];
            let (n, addr) = socket.recv_from(&mut buf).await?;
            buf.truncate(n);
            Ok((Bytes::from(buf), addr))
        }

        if let Some(socket_v6) = socket_v6 {
            tokio::select! {
                res = read_pkt(socket_v4, max_pkt_size) => res,
                res = read_pkt(socket_v6, max_pkt_size) => res,
            }
        } else {
            read_pkt(socket_v4, max_pkt_size).await
        }
    }
}

impl Drop for UdpSession {
    fn drop(&mut self) {
        let _ = self.cancel.take().unwrap().send(());
    }
}

#[derive(Clone)]
struct Authenticated(Arc<AuthenticatedInner>);

struct AuthenticatedInner {
    uuid: AtomicCell<Option<Uuid>>,
    broadcast: Mutex<Vec<Waker>>,
}

impl Authenticated {
    fn new() -> Self {
        Self(Arc::new(AuthenticatedInner {
            uuid: AtomicCell::new(None),
            broadcast: Mutex::new(Vec::new()),
        }))
    }

    fn set(&self, uuid: Uuid) {
        self.0.uuid.store(Some(uuid));

        for waker in self.0.broadcast.lock().drain(..) {
            waker.wake();
        }
    }

    fn get(&self) -> Option<Uuid> {
        self.0.uuid.load()
    }
}

impl Future for Authenticated {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.get().is_some() {
            Poll::Ready(())
        } else {
            self.0.broadcast.lock().push(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl Error {
    fn is_locally_closed(&self) -> bool {
        matches!(self, Self::Connection(ConnectionError::LocallyClosed))
    }

    fn is_timeout_closed(&self) -> bool {
        matches!(self, Self::Connection(ConnectionError::TimedOut))
    }
}
