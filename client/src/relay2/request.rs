use super::{Address, BiStream, Connection};
use bytes::Bytes;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use std::{
    future::Future,
    ops::Deref,
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
    conn: Arc<AsyncMutex<Connection>>,
    mut req_rx: MpscReceiver<Request>,
) -> (impl Future<Output = ()>, Wait) {
    let (reg, count) = Register::new();

    let listen = async move {
        while let Some(req) = req_rx.recv().await {
            tokio::spawn(process_request(conn.clone(), req, reg.clone()));
        }
    };

    (listen, count)
}

async fn process_request(conn: Arc<AsyncMutex<Connection>>, req: Request, reg: Register) {
    if let Ok(lock) = time::timeout(Duration::from_secs(5), conn.lock()).await {
        let conn = lock.deref().clone();
        drop(lock);

        todo!()
    } else {
        log::warn!("timeout");
    }

    // drop the register explicitly
    drop(reg)
}

pub enum Request {
    Connect {
        addr: Address,
        tx: ConnectResponseSender,
    },
    Associate {
        assoc_id: u32,
        pkt_send_rx: AssociateSendPacketReceiver,
        pkt_receive_tx: AssociateRecvPacketSender,
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
        let reg = Self(Arc::clone(&self.0));

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
            // safety: the `Arc` must be owned by at least one scope (`listen_request`)
            *self.0.upgrade().unwrap().lock() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
