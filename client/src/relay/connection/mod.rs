use super::{Address, RelayError, TaskCount, UdpMode};
use bytes::Bytes;
use futures_util::StreamExt;
use parking_lot::Mutex;
use quinn::{
    Connecting, Connection as QuinnConnection, ConnectionError, Datagrams, IncomingUniStreams,
    NewConnection,
};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{sync::mpsc::Sender, time};
use tuic_protocol::Command;

mod dispatch;
mod task;

#[derive(Clone)]
pub struct Connection {
    controller: QuinnConnection,
    udp_mode: UdpMode,
    udp_sessions: Arc<UdpSessionMap>,
    is_closed: IsClosed,
}

pub type UdpSessionMap = Mutex<HashMap<u32, Sender<(Bytes, Address)>>>;

impl Connection {
    pub async fn init(
        conn: Connecting,
        token_digest: [u8; 32],
        udp_mode: UdpMode,
        reduce_rtt: bool,
    ) -> Result<Self, RelayError> {
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

        let udp_sessions = Arc::new(Mutex::new(HashMap::new()));
        let is_closed = IsClosed::new();

        let conn = Self {
            controller: connection,
            udp_mode,
            udp_sessions,
            is_closed,
        };

        tokio::spawn(Self::authenticate(conn.clone(), token_digest));

        match udp_mode {
            UdpMode::Native => tokio::spawn(Self::listen_datagrams(conn.clone(), datagrams)),
            UdpMode::Quic => tokio::spawn(Self::listen_uni_streams(conn.clone(), uni_streams)),
        };

        Ok(conn)
    }

    pub fn is_closed(&self) -> bool {
        self.is_closed.check()
    }

    pub fn start_heartbeat(&self, task_count: TaskCount) {
        async fn heartbeat(conn: &QuinnConnection) -> Result<(), RelayError> {
            let mut stream = conn.open_uni().await?;
            let heartbeat = Command::new_heartbeat();
            heartbeat.write_to(&mut stream).await?;
            Ok(())
        }

        let is_closed = self.is_closed.clone();
        let mut interval = time::interval(Duration::from_secs(10));
        let conn = self.controller.clone();

        tokio::spawn(async move {
            while !tokio::select! {
                _ = is_closed.clone() => true,
                _ = interval.tick() => false,
            } {
                if !task_count.is_zero() {
                    match heartbeat(&conn).await {
                        Ok(()) => log::debug!("[relay] [connection] [heartbeat]"),
                        Err(err) => log::error!("[relay] [connection] [heartbeat] {err}"),
                    }
                }
            }
        });
    }

    async fn authenticate(self, token_digest: [u8; 32]) {
        async fn send_authenticate(
            conn: QuinnConnection,
            token_digest: [u8; 32],
        ) -> Result<(), RelayError> {
            let mut stream = conn.open_uni().await?;
            let cmd = Command::new_authenticate(token_digest);
            cmd.write_to(&mut stream).await?;
            Ok(())
        }

        match send_authenticate(self.controller, token_digest).await {
            Ok(()) => log::debug!("[relay] [connection] [authentication]"),
            Err(err) => {
                self.is_closed.set_closed();
                log::error!("[relay] [connection] [authentication] {err}");
            }
        }
    }

    async fn listen_uni_streams(self, uni_streams: IncomingUniStreams) {
        async fn listen(
            conn: Connection,
            mut uni_streams: IncomingUniStreams,
        ) -> Result<(), RelayError> {
            while let Some(stream) = uni_streams.next().await {
                let stream = stream?;
                let conn = conn.clone();

                tokio::spawn(async move {
                    match conn.process_incoming_uni_stream(stream).await {
                        Ok(()) => (),
                        Err(err) => {
                            log::warn!("[relay] [connection] [incoming] {err}");
                        }
                    }
                });
            }

            Err(RelayError::Connection(ConnectionError::LocallyClosed))
        }

        let is_closed = self.is_closed.clone();

        match listen(self, uni_streams).await {
            Ok(())
            | Err(RelayError::Connection(ConnectionError::LocallyClosed))
            | Err(RelayError::Connection(ConnectionError::TimedOut)) => {}
            Err(err) => log::error!("[relay] [connection] {err}"),
        }

        is_closed.set_closed();
    }

    async fn listen_datagrams(self, datagrams: Datagrams) {
        async fn listen(conn: Connection, mut datagrams: Datagrams) -> Result<(), RelayError> {
            while let Some(datagram) = datagrams.next().await {
                let datagram = datagram?;
                let conn = conn.clone();

                tokio::spawn(async move {
                    match conn.process_incoming_datagram(datagram).await {
                        Ok(()) => (),
                        Err(err) => {
                            log::warn!("[relay] [connection] [incoming] {err}");
                        }
                    }
                });
            }

            Err(RelayError::Connection(ConnectionError::LocallyClosed))
        }

        let is_closed = self.is_closed.clone();

        match listen(self, datagrams).await {
            Ok(())
            | Err(RelayError::Connection(ConnectionError::LocallyClosed))
            | Err(RelayError::Connection(ConnectionError::TimedOut)) => {}
            Err(err) => log::error!("[relay] [connection] {err}"),
        }

        is_closed.set_closed();
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

    fn set_closed(&self) {
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
