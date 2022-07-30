use self::{
    authenticate::IsAuthenticated,
    dispatch::DispatchError,
    udp::{RecvPacketReceiver, UdpPacketFrom, UdpPacketSource, UdpSessionMap},
};
use futures_util::StreamExt;
use parking_lot::Mutex;
use quinn::{
    Connecting, Connection as QuinnConnection, ConnectionError, Datagrams, IncomingBiStreams,
    IncomingUniStreams, NewConnection,
};
use std::{
    collections::HashSet,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::time;

mod authenticate;
mod dispatch;
mod task;
mod udp;

#[derive(Clone)]
pub struct Connection {
    controller: QuinnConnection,
    udp_packet_from: UdpPacketFrom,
    udp_sessions: Arc<UdpSessionMap>,
    token: Arc<HashSet<[u8; 32]>>,
    is_authenticated: IsAuthenticated,
}

impl Connection {
    pub async fn handle(
        conn: Connecting,
        token: Arc<HashSet<[u8; 32]>>,
        auth_timeout: Duration,
        max_pkt_size: usize,
    ) {
        let rmt_addr = conn.remote_address();

        match conn.await {
            Ok(NewConnection {
                connection,
                uni_streams,
                bi_streams,
                datagrams,
                ..
            }) => {
                log::debug!("[{rmt_addr}] [establish]");

                let (udp_sessions, recv_pkt_rx) = UdpSessionMap::new(max_pkt_size);
                let is_closed = IsClosed::new();
                let is_authed = IsAuthenticated::new(is_closed.clone());

                let conn = Self {
                    controller: connection,
                    udp_packet_from: UdpPacketFrom::new(),
                    udp_sessions: Arc::new(udp_sessions),
                    token,
                    is_authenticated: is_authed,
                };

                let res = tokio::select! {
                    res = Self::listen_uni_streams(conn.clone(), uni_streams) => res,
                    res = Self::listen_bi_streams(conn.clone(), bi_streams) => res,
                    res = Self::listen_datagrams(conn.clone(), datagrams) => res,
                    res = Self::listen_received_udp_packet(conn.clone(), recv_pkt_rx) => res,
                    Err(err) = Self::handle_authentication_timeout(conn, auth_timeout) => Err(err),
                };

                match res {
                    Ok(()) => unreachable!(),
                    Err(err) => {
                        is_closed.set_closed();

                        match err {
                            ConnectionError::TimedOut => {
                                log::debug!("[{rmt_addr}] [disconnect] [connection timeout]")
                            }
                            ConnectionError::LocallyClosed => {
                                log::debug!("[{rmt_addr}] [disconnect] [locally closed]")
                            }
                            err => log::error!("[{rmt_addr}] [disconnect] {err}"),
                        }
                    }
                }
            }
            Err(err) => log::error!("[{rmt_addr}] {err}"),
        }
    }

    async fn listen_uni_streams(
        self,
        mut uni_streams: IncomingUniStreams,
    ) -> Result<(), ConnectionError> {
        while let Some(stream) = uni_streams.next().await {
            let stream = stream?;
            let conn = self.clone();

            tokio::spawn(async move {
                match conn.process_uni_stream(stream).await {
                    Ok(()) => {}
                    Err(err) => {
                        conn.controller
                            .close(err.as_error_code(), err.to_string().as_bytes());

                        let rmt_addr = conn.controller.remote_address();
                        log::error!("[{rmt_addr}] {err}");
                    }
                }
            });
        }

        Err(ConnectionError::LocallyClosed)
    }

    async fn listen_bi_streams(
        self,
        mut bi_streams: IncomingBiStreams,
    ) -> Result<(), ConnectionError> {
        while let Some(stream) = bi_streams.next().await {
            let (send, recv) = stream?;
            let conn = self.clone();

            tokio::spawn(async move {
                match conn.process_bi_stream(send, recv).await {
                    Ok(()) => {}
                    Err(err) => {
                        conn.controller
                            .close(err.as_error_code(), err.to_string().as_bytes());

                        let rmt_addr = conn.controller.remote_address();
                        log::error!("[{rmt_addr}] {err}");
                    }
                }
            });
        }

        Err(ConnectionError::LocallyClosed)
    }

    async fn listen_datagrams(self, mut datagrams: Datagrams) -> Result<(), ConnectionError> {
        while let Some(datagram) = datagrams.next().await {
            let datagram = datagram?;
            let conn = self.clone();

            tokio::spawn(async move {
                match conn.process_datagram(datagram).await {
                    Ok(()) => {}
                    Err(err) => {
                        conn.controller
                            .close(err.as_error_code(), err.to_string().as_bytes());

                        let rmt_addr = conn.controller.remote_address();
                        log::error!("[{rmt_addr}] {err}");
                    }
                }
            });
        }

        Err(ConnectionError::LocallyClosed)
    }

    async fn listen_received_udp_packet(
        self,
        mut recv_pkt_rx: RecvPacketReceiver,
    ) -> Result<(), ConnectionError> {
        while let Some((assoc_id, pkt, addr)) = recv_pkt_rx.recv().await {
            let conn = self.clone();

            tokio::spawn(async move {
                match conn.process_received_udp_packet(assoc_id, pkt, addr).await {
                    Ok(()) => {}
                    Err(err) => {
                        conn.controller
                            .close(err.as_error_code(), err.to_string().as_bytes());

                        let rmt_addr = conn.controller.remote_address();
                        log::error!("[{rmt_addr}] {err}");
                    }
                }
            });
        }

        Err(ConnectionError::LocallyClosed)
    }

    async fn handle_authentication_timeout(self, timeout: Duration) -> Result<(), ConnectionError> {
        let is_timeout = tokio::select! {
            _ = self.is_authenticated.clone() => false,
            () = time::sleep(timeout) => true,
        };

        if !is_timeout {
            Ok(())
        } else {
            let err = DispatchError::AuthenticationTimeout;

            self.controller
                .close(err.as_error_code(), err.to_string().as_bytes());
            self.is_authenticated.wake();

            let rmt_addr = self.controller.remote_address();
            log::error!("[{rmt_addr}] {err}");

            Err(ConnectionError::LocallyClosed)
        }
    }
}

#[derive(Clone)]
pub struct IsClosed(Arc<IsClosedInner>);

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
