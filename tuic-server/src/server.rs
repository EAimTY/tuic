use crate::{config::Config, utils::UdpRelayMode, Error};
use bytes::Bytes;
use crossbeam_utils::atomic::AtomicCell;
use parking_lot::Mutex;
use quinn::{Connecting, Connection as QuinnConnection, Endpoint, RecvStream, SendStream, VarInt};
use std::{
    collections::{hash_map::Entry, HashMap},
    future::Future,
    io::{Error as IoError, ErrorKind},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
use tokio::{
    io::{self, AsyncWriteExt},
    net::{self, TcpStream, UdpSocket},
    sync::{
        oneshot::{self, Receiver, Sender},
        Mutex as AsyncMutex,
    },
};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tuic::Address;
use tuic_quinn::{side, Connect, Connection as Model, Packet, Task};

pub struct Server {
    ep: Endpoint,
    token: Arc<[u8]>,
    udp_relay_ipv6: bool,
    zero_rtt_handshake: bool,
}

impl Server {
    pub fn init(cfg: Config) -> Result<Self, Error> {
        todo!()
    }

    pub async fn start(&self) {
        loop {
            let conn = self.ep.accept().await.unwrap();
            tokio::spawn(Connection::init(
                conn,
                self.token.clone(),
                self.udp_relay_ipv6,
                self.zero_rtt_handshake,
            ));
        }
    }
}

#[derive(Clone)]
struct Connection {
    inner: QuinnConnection,
    model: Model<side::Server>,
    token: Arc<[u8]>,
    udp_relay_ipv6: bool,
    is_authed: IsAuthed,
    udp_sessions: Arc<AsyncMutex<HashMap<u16, UdpSession>>>,
    udp_relay_mode: Arc<AtomicCell<Option<UdpRelayMode>>>,
}

impl Connection {
    pub async fn init(
        conn: Connecting,
        token: Arc<[u8]>,
        udp_relay_ipv6: bool,
        zero_rtt_handshake: bool,
    ) {
        match Self::handshake(conn, token, udp_relay_ipv6, zero_rtt_handshake).await {
            Ok(conn) => loop {
                if conn.is_closed() {
                    break;
                }

                match conn.accept().await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                }
            },
            Err(err) => eprintln!("{err}"),
        }
    }

    async fn handshake(
        conn: Connecting,
        token: Arc<[u8]>,
        udp_relay_ipv6: bool,
        zero_rtt_handshake: bool,
    ) -> Result<Self, Error> {
        let conn = if zero_rtt_handshake {
            match conn.into_0rtt() {
                Ok((conn, _)) => conn,
                Err(conn) => {
                    eprintln!("0-RTT handshake failed, fallback to 1-RTT handshake");
                    conn.await?
                }
            }
        } else {
            conn.await?
        };

        Ok(Self {
            inner: conn.clone(),
            model: Model::<side::Server>::new(conn),
            token,
            udp_relay_ipv6,
            is_authed: IsAuthed::new(),
            udp_sessions: Arc::new(AsyncMutex::new(HashMap::new())),
            udp_relay_mode: Arc::new(AtomicCell::new(None)),
        })
    }

    async fn accept(&self) -> Result<(), Error> {
        tokio::select! {
            res = self.inner.accept_uni() => tokio::spawn(self.clone().handle_uni_stream(res?)),
            res = self.inner.accept_bi() => tokio::spawn(self.clone().handle_bi_stream(res?)),
            res = self.inner.read_datagram() => tokio::spawn(self.clone().handle_datagram(res?)),
        };

        Ok(())
    }

    async fn handle_uni_stream(self, recv: RecvStream) {
        async fn pre_process(conn: &Connection, recv: RecvStream) -> Result<Task, Error> {
            let task = conn.model.accept_uni_stream(recv).await?;

            if let Task::Authenticate(token) = &task {
                if conn.is_authed() {
                    return Err(Error::DuplicatedAuth);
                } else {
                    let mut buf = [0; 32];
                    conn.inner
                        .export_keying_material(&mut buf, &conn.token, &conn.token)
                        .map_err(|_| Error::ExportKeyingMaterial)?;

                    if token == &buf {
                        conn.set_authed();
                    } else {
                        return Err(Error::AuthFailed);
                    }
                }
            }

            tokio::select! {
                () = conn.authed() => {}
                err = conn.inner.closed() => Err(err)?,
            };

            let same_pkt_src = matches!(task, Task::Packet(_))
                && matches!(conn.get_udp_relay_mode(), Some(UdpRelayMode::Native));
            if same_pkt_src {
                return Err(Error::UnexpectedPacketSource);
            }

            Ok(task)
        }

        match pre_process(&self, recv).await {
            Ok(Task::Packet(pkt)) => {
                self.set_udp_relay_mode(UdpRelayMode::Quic);
                match self.handle_packet(pkt).await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                }
            }
            Ok(Task::Dissociate(assoc_id)) => match self.handle_dissociate(assoc_id).await {
                Ok(()) => {}
                Err(err) => eprintln!("{err}"),
            },
            Ok(_) => unreachable!(),
            Err(err) => {
                eprintln!("{err}");
                self.inner.close(VarInt::from_u32(0), b"");
                return;
            }
        }
    }

    async fn handle_bi_stream(self, (send, recv): (SendStream, RecvStream)) {
        async fn pre_process(
            conn: &Connection,
            send: SendStream,
            recv: RecvStream,
        ) -> Result<Task, Error> {
            let task = conn.model.accept_bi_stream(send, recv).await?;

            tokio::select! {
                () = conn.authed() => {}
                err = conn.inner.closed() => Err(err)?,
            };

            Ok(task)
        }

        match pre_process(&self, send, recv).await {
            Ok(Task::Connect(conn)) => match self.handle_connect(conn).await {
                Ok(()) => {}
                Err(err) => eprintln!("{err}"),
            },
            Ok(_) => unreachable!(),
            Err(err) => {
                eprintln!("{err}");
                self.inner.close(VarInt::from_u32(0), b"");
                return;
            }
        }
    }

    async fn handle_datagram(self, dg: Bytes) {
        async fn pre_process(conn: &Connection, dg: Bytes) -> Result<Task, Error> {
            let task = conn.model.accept_datagram(dg)?;

            tokio::select! {
                () = conn.authed() => {}
                err = conn.inner.closed() => Err(err)?,
            };

            let same_pkt_src = matches!(task, Task::Packet(_))
                && matches!(conn.get_udp_relay_mode(), Some(UdpRelayMode::Quic));
            if same_pkt_src {
                return Err(Error::UnexpectedPacketSource);
            }

            Ok(task)
        }

        match pre_process(&self, dg).await {
            Ok(Task::Packet(pkt)) => {
                self.set_udp_relay_mode(UdpRelayMode::Native);
                match self.handle_packet(pkt).await {
                    Ok(()) => {}
                    Err(err) => eprintln!("{err}"),
                }
            }
            Ok(Task::Heartbeat) => {}
            Ok(_) => unreachable!(),
            Err(err) => {
                eprintln!("{err}");
                self.inner.close(VarInt::from_u32(0), b"");
                return;
            }
        }
    }

    async fn handle_connect(&self, conn: Connect) -> Result<(), Error> {
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
            let _ = conn.shutdown().await;
            let _ = stream.shutdown().await;
            res?;
            Ok(())
        } else {
            let _ = conn.compat().shutdown().await;
            Err(last_err
                .unwrap_or_else(|| IoError::new(ErrorKind::NotFound, "no address resolved")))?
        }
    }

    async fn handle_packet(&self, pkt: Packet) -> Result<(), Error> {
        let Some((pkt, addr, assoc_id)) = pkt.accept().await? else {
            return Ok(());
        };

        let (socket_v4, socket_v6) = match self.udp_sessions.lock().await.entry(assoc_id) {
            Entry::Occupied(mut entry) => {
                let session = entry.get_mut();
                (session.socket_v4.clone(), session.socket_v6.clone())
            }
            Entry::Vacant(entry) => {
                let session = entry
                    .insert(UdpSession::new(assoc_id, self.clone(), self.udp_relay_ipv6).await?);
                (session.socket_v4.clone(), session.socket_v6.clone())
            }
        };

        let Some(socket_addr) = resolve_dns(&addr).await?.next() else {
            Err(IoError::new(ErrorKind::NotFound, "no address resolved"))?
        };

        let socket = match socket_addr {
            SocketAddr::V4(_) => socket_v4,
            SocketAddr::V6(_) => {
                socket_v6.ok_or_else(|| Error::UdpRelayIpv6Disabled(addr, socket_addr))?
            }
        };

        socket.send_to(&pkt, socket_addr).await?;

        Ok(())
    }

    async fn handle_dissociate(&self, assoc_id: u16) -> Result<(), Error> {
        self.udp_sessions.lock().await.remove(&assoc_id);
        Ok(())
    }

    fn set_authed(&self) {
        self.is_authed.set_authed();
    }

    fn is_authed(&self) -> bool {
        self.is_authed.is_authed()
    }

    fn authed(&self) -> IsAuthed {
        self.is_authed.clone()
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
        let socket_v4 =
            Arc::new(UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))).await?);
        let socket_v6 = if udp_relay_ipv6 {
            Some(Arc::new(
                UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).await?,
            ))
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
        todo!()
    }
}

impl Drop for UdpSession {
    fn drop(&mut self) {
        let _ = self.cancel.take().unwrap().send(());
    }
}

#[derive(Clone)]
struct IsAuthed {
    is_authed: Arc<AtomicBool>,
    broadcast: Arc<Mutex<Vec<Waker>>>,
}

impl IsAuthed {
    fn new() -> Self {
        Self {
            is_authed: Arc::new(AtomicBool::new(false)),
            broadcast: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn set_authed(&self) {
        self.is_authed.store(true, Ordering::Release);

        for waker in self.broadcast.lock().drain(..) {
            waker.wake();
        }
    }

    fn is_authed(&self) -> bool {
        self.is_authed.load(Ordering::Relaxed)
    }
}

impl Future for IsAuthed {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.is_authed.load(Ordering::Relaxed) {
            Poll::Ready(())
        } else {
            self.broadcast.lock().push(cx.waker().clone());
            Poll::Pending
        }
    }
}
