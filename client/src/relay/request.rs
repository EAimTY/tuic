use super::{stream::BiStream, Address, Connection};
use bytes::Bytes;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    future::Future,
    pin::Pin,
    sync::{Arc, Weak},
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{
    sync::{
        mpsc::{self, Receiver as MpscReceiver, Sender as MpscSender},
        oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
        Mutex as AsyncMutex,
    },
    time,
};

pub fn listen_requests(
    conn: Arc<AsyncMutex<Option<Connection>>>,
    mut req_rx: MpscReceiver<Request>,
    timeout: u64,
) -> (impl Future<Output = ()>, Wait) {
    let (reg, count) = Register::new();

    let listen = async move {
        while let Some(req) = req_rx.recv().await {
            tokio::spawn(process_request(conn.clone(), req, timeout, reg.clone()));
        }
    };

    (listen, count)
}

async fn process_request(
    conn: Arc<AsyncMutex<Option<Connection>>>,
    req: Request,
    timeout: u64,
    _reg: Register,
) {
    log::info!("[relay] [task] {req}");

    // try to get the current connection
    if let Ok(lock) = time::timeout(Duration::from_millis(timeout), conn.lock()).await {
        let conn = lock.as_ref().unwrap().clone(); // safety: there must be a connection if the lock is aquirable
        drop(lock);

        match req {
            Request::Connect { addr, tx } => conn.clone().handle_connect(addr, tx).await,
            Request::Associate {
                assoc_id,
                mut pkt_send_rx,
                pkt_recv_tx,
            } => {
                conn.udp_sessions().insert(assoc_id, pkt_recv_tx);
                while let Some((pkt, addr)) = pkt_send_rx.recv().await {
                    tokio::spawn(conn.clone().handle_packet_to(
                        assoc_id,
                        pkt,
                        addr,
                        conn.udp_relay_mode(),
                    ));
                }

                log::info!("[relay] [task] [dissociate] [{assoc_id}]");
                conn.clone().udp_sessions().remove(&assoc_id);
                conn.handle_dissociate(assoc_id).await;
            }
        }
    } else {
        log::warn!("[relay] [task] {req} [timeout]");
    }
}

pub enum Request {
    Connect {
        addr: Address,
        tx: ConnectResponseSender,
    },
    Associate {
        assoc_id: u32,
        pkt_send_rx: AssociateSendPacketReceiver,
        pkt_recv_tx: AssociateRecvPacketSender,
    },
}

type ConnectResponseSender = OneshotSender<BiStream>;
type ConnectResponseReceiver = OneshotReceiver<BiStream>;
type AssociateSendPacketSender = MpscSender<(Bytes, Address)>;
type AssociateSendPacketReceiver = MpscReceiver<(Bytes, Address)>;
type AssociateRecvPacketSender = MpscSender<(Bytes, Address)>;
type AssociateRecvPacketReceiver = MpscReceiver<(Bytes, Address)>;

impl Request {
    pub fn new_connect(addr: Address) -> (Self, ConnectResponseReceiver) {
        let (tx, rx) = oneshot::channel();
        (Request::Connect { addr, tx }, rx)
    }

    pub fn new_associate() -> (Self, AssociateSendPacketSender, AssociateRecvPacketReceiver) {
        let assoc_id = get_random_u32();
        let (pkt_send_tx, pkt_send_rx) = mpsc::channel(1);
        let (pkt_recv_tx, pkt_recv_rx) = mpsc::channel(1);

        (
            Self::Associate {
                assoc_id,
                pkt_send_rx,
                pkt_recv_tx,
            },
            pkt_send_tx,
            pkt_recv_rx,
        )
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Request::Connect { addr, .. } => write!(f, "[connect] [{addr}]"),
            Request::Associate { assoc_id, .. } => write!(f, "[associate] [{assoc_id}]"),
        }
    }
}

static RNG: Lazy<Mutex<StdRng>> = Lazy::new(|| Mutex::new(StdRng::from_entropy()));

fn get_random_u32() -> u32 {
    RNG.lock().next_u32()
}

pub struct Register(Arc<Mutex<Option<Waker>>>);

impl Register {
    pub fn new() -> (Self, Wait) {
        let reg = Self(Arc::new(Mutex::new(None)));
        let count = Wait(Arc::downgrade(&reg.0));
        (reg, count)
    }
}

impl Clone for Register {
    fn clone(&self) -> Self {
        let reg = Self(self.0.clone());

        // wake the `Wait` hold by `guard_connection`
        if let Some(waker) = self.0.lock().take() {
            waker.wake();
        }

        reg
    }
}

#[derive(Clone)]
pub struct Wait(Weak<Mutex<Option<Waker>>>);

impl Future for Wait {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0.strong_count() > 1 {
            // there is a request waiting
            Poll::Ready(())
        } else {
            // there is no request waiting, pend the task
            if let Some(reg) = self.0.upgrade() {
                *reg.lock() = Some(cx.waker().clone());
            }
            Poll::Pending
        }
    }
}
