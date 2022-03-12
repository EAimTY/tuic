use super::Address;
use bytes::Bytes;
use parking_lot::Mutex;
use quinn::{RecvStream, SendStream};
use rand::{rngs::StdRng, RngCore, SeedableRng};
use std::lazy::SyncLazy;
use tokio::sync::{
    mpsc::{self, Receiver as MpscReceiver, Sender as MpscSender},
    oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
};

pub enum Request {
    Connect {
        addr: Address,
        tx: OneshotSender<Option<(SendStream, RecvStream)>>,
    },
    Associate {
        assoc_id: u32,
        pkt_send_rx: MpscReceiver<(Bytes, Address)>,
        pkt_receive_tx: MpscSender<(Bytes, Address)>,
    },
}

impl Request {
    pub fn new_connect(addr: Address) -> (Self, OneshotReceiver<Option<(SendStream, RecvStream)>>) {
        let (tx, rx) = oneshot::channel();
        (Request::Connect { addr, tx }, rx)
    }

    pub fn new_associate() -> (
        Self,
        MpscSender<(Bytes, Address)>,
        MpscReceiver<(Bytes, Address)>,
    ) {
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

static RNG: SyncLazy<Mutex<StdRng>> = SyncLazy::new(|| Mutex::new(StdRng::from_entropy()));

fn get_random_u32() -> u32 {
    RNG.lock().next_u32()
}
