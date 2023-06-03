use self::{authenticated::Authenticated, udp_session::UdpSession};
use crate::{error::Error, utils::UdpRelayMode};
use crossbeam_utils::atomic::AtomicCell;
use parking_lot::Mutex;
use quinn::{Connecting, Connection as QuinnConnection, VarInt};
use register_count::Counter;
use std::{
    collections::HashMap,
    sync::{atomic::AtomicU32, Arc},
    time::Duration,
};
use tokio::time;
use tuic_quinn::{side, Authenticate, Connection as Model};
use uuid::Uuid;

mod authenticated;
mod handle_stream;
mod handle_task;
mod udp_session;

pub const ERROR_CODE: VarInt = VarInt::from_u32(0);
pub const DEFAULT_CONCURRENT_STREAMS: u32 = 32;

#[derive(Clone)]
pub struct Connection {
    inner: QuinnConnection,
    model: Model<side::Server>,
    users: Arc<HashMap<Uuid, Box<[u8]>>>,
    udp_relay_ipv6: bool,
    auth: Authenticated,
    task_negotiation_timeout: Duration,
    udp_sessions: Arc<Mutex<HashMap<u16, UdpSession>>>,
    udp_relay_mode: Arc<AtomicCell<Option<UdpRelayMode>>>,
    max_external_pkt_size: usize,
    remote_uni_stream_cnt: Counter,
    remote_bi_stream_cnt: Counter,
    max_concurrent_uni_streams: Arc<AtomicU32>,
    max_concurrent_bi_streams: Arc<AtomicU32>,
}

#[allow(clippy::too_many_arguments)]
impl Connection {
    pub async fn handle(
        conn: Connecting,
        users: Arc<HashMap<Uuid, Box<[u8]>>>,
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
                log::info!(
                    "[{id:#010x}] [{addr}] [{user}] connection established",
                    id = conn.id(),
                    user = conn.auth,
                );

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
                        Err(err) if err.is_trivial() => {
                            log::debug!(
                                "[{id:#010x}] [{addr}] [{user}] {err}",
                                id = conn.id(),
                                user = conn.auth,
                            );
                        }
                        Err(err) => log::warn!(
                            "[{id:#010x}] [{addr}] [{user}] connection error: {err}",
                            id = conn.id(),
                            user = conn.auth,
                        ),
                    }
                }
            }
            Err(err) if err.is_trivial() => {
                log::debug!(
                    "[{id:#010x}] [{addr}] [unauthenticated] {err}",
                    id = u32::MAX,
                );
            }
            Err(err) => {
                log::warn!(
                    "[{id:#010x}] [{addr}] [unauthenticated] {err}",
                    id = u32::MAX,
                )
            }
        }
    }

    fn new(
        conn: QuinnConnection,
        users: Arc<HashMap<Uuid, Box<[u8]>>>,
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
            udp_sessions: Arc::new(Mutex::new(HashMap::new())),
            udp_relay_mode: Arc::new(AtomicCell::new(None)),
            max_external_pkt_size,
            remote_uni_stream_cnt: Counter::new(),
            remote_bi_stream_cnt: Counter::new(),
            max_concurrent_uni_streams: Arc::new(AtomicU32::new(DEFAULT_CONCURRENT_STREAMS)),
            max_concurrent_bi_streams: Arc::new(AtomicU32::new(DEFAULT_CONCURRENT_STREAMS)),
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
            log::warn!(
                "[{id:#010x}] [{addr}] [unauthenticated] [authenticate] timeout",
                id = self.id(),
                addr = self.inner.remote_address(),
            );
            self.close();
        }
    }

    async fn collect_garbage(self, gc_interval: Duration, gc_lifetime: Duration) {
        loop {
            time::sleep(gc_interval).await;

            if self.is_closed() {
                break;
            }

            log::debug!(
                "[{id:#010x}] [{addr}] [{user}] packet fragment garbage collecting event",
                id = self.id(),
                addr = self.inner.remote_address(),
                user = self.auth,
            );
            self.model.collect_garbage(gc_lifetime);
        }
    }

    fn id(&self) -> u32 {
        self.inner.stable_id() as u32
    }

    fn is_closed(&self) -> bool {
        self.inner.close_reason().is_some()
    }

    fn close(&self) {
        self.inner.close(ERROR_CODE, &[]);
    }
}
