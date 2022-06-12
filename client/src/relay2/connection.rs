use super::{
    incoming::{self, Receiver as IncomingReceiver, Sender as IncomingSender},
    request::Wait as WaitRequest,
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
    ops::DerefMut,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
use tokio::{
    net,
    sync::{
        mpsc::Sender as MpscSender,
        oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
        Mutex as AsyncMutex, OwnedMutexGuard,
    },
};

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
        let mut is_closed;

        loop {
            // start the procedure only if there is a request waiting
            wait_req.clone().await;

            // try to establish a new connection
            let (new_conn, dg, uni);
            (new_conn, is_closed, dg, uni) = match Connection::connect(&config).await {
                Ok(conn) => conn,
                Err(err) => {
                    log::error!("{err}");
                    continue;
                }
            };

            // renew the connection mutex
            let mut lock = lock.take().unwrap(); // safety: the mutex must be locked before
            *lock.deref_mut() = new_conn;

            // send the incoming streams to `incoming::listen_incoming`
            match next_incoming_tx {
                UdpRelayMode::Native(incoming_tx) => {
                    let (tx, rx) = incoming::channel::<Datagrams>();
                    incoming_tx.send(dg, rx, is_closed.clone());
                    next_incoming_tx = UdpRelayMode::Native(tx);
                }
                UdpRelayMode::Quic(incoming_tx) => {
                    let (tx, rx) = incoming::channel::<IncomingUniStreams>();
                    incoming_tx.send(uni, rx, is_closed.clone());
                    next_incoming_tx = UdpRelayMode::Quic(tx);
                }
            };

            // connection established, drop the lock implicitly
            break;
        }

        // wait for the connection to be closed, lock the mutex
        is_closed.await;
        lock = Some(conn.clone().lock_owned().await);
    }
}

#[derive(Clone)]
pub struct Connection {
    conn: QuinnConnection,
}

impl Connection {
    async fn connect(
        config: &ConnectionConfig,
    ) -> Result<(Self, IsClosed, Datagrams, IncomingUniStreams)> {
        let (addrs, name) = match &config.server_addr {
            ServerAddr::SocketAddr { addr, name } => Ok((vec![*addr], name)),
            ServerAddr::DomainAddr { domain, port } => net::lookup_host((domain.as_str(), *port))
                .await
                .map(|res| (res.collect(), domain)),
        }?;

        let mut conn = None;

        for addr in addrs {
            match Self::connect_addr(config.quinn_config.clone(), addr, name, config.reduce_rtt)
                .await
            {
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
        config: ClientConfig,
        addr: SocketAddr,
        name: &str,
        reduce_rtt: bool,
    ) -> Result<(Self, IsClosed, Datagrams, IncomingUniStreams)> {
        let bind_addr = match addr {
            SocketAddr::V4(_) => SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
            SocketAddr::V6(_) => SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
        };

        let conn = Endpoint::client(bind_addr)?
            .connect_with(config, addr, name)
            .map_err(|err| Error::new(ErrorKind::Other, err))?;

        let NewConnection {
            connection,
            datagrams,
            uni_streams,
            ..
        } = if reduce_rtt {
            match conn.into_0rtt() {
                Ok((conn, _)) => conn,
                Err(conn) => conn.await?,
            }
        } else {
            conn.await?
        };

        let (conn, is_closed) = Self::new(connection).await?;
        Ok((conn, is_closed, datagrams, uni_streams))
    }

    async fn new(conn: QuinnConnection) -> Result<(Self, IsClosed)> {
        // send auth
        // heartbeat
        todo!();
        let is_closed = IsClosed::new();
        Ok((Self { conn }, is_closed))
    }
}

pub struct ConnectionConfig {
    quinn_config: ClientConfig,
    server_addr: ServerAddr,
    token_digest: [u8; 32],
    heartbeat_interval: u64,
    reduce_rtt: bool,
}

impl ConnectionConfig {
    pub fn new(
        quinn_config: ClientConfig,
        server_addr: ServerAddr,
        token_digest: [u8; 32],
        heartbeat_interval: u64,
        reduce_rtt: bool,
    ) -> Self {
        Self {
            quinn_config,
            server_addr,
            token_digest,
            heartbeat_interval,
            reduce_rtt,
        }
    }
}

pub type UdpSessionMap = Mutex<HashMap<u32, MpscSender<(Bytes, Address)>>>;

#[derive(Clone)]
pub struct IsClosed(Arc<IsClosedInner>);

struct IsClosedInner {
    is_closed: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl IsClosed {
    pub fn new() -> Self {
        Self(Arc::new(IsClosedInner {
            is_closed: AtomicBool::new(false),
            waker: Mutex::new(None),
        }))
    }

    pub fn set_closed(&self) {
        self.0.is_closed.store(true, Ordering::Release);

        if let Some(waker) = self.0.waker.lock().take() {
            waker.wake();
        }
    }

    fn check(&self) -> bool {
        self.0.is_closed.load(Ordering::Acquire)
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
