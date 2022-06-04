use super::{
    incoming::{NextDatagramsSender, NextIncomingUniStreamsSender},
    Address, UdpRelayMode,
};
use bytes::Bytes;
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{
    mpsc::Sender as MpscSender,
    oneshot::{self, Receiver as OneshotReceiver, Sender as OneshotSender},
    Mutex as AsyncMutex, OwnedMutexGuard,
};

pub async fn guard_connection(
    conn: Arc<AsyncMutex<Connection>>,
    lock: OwnedMutexGuard<Connection>,
    udp_relay_mode: UdpRelayMode,
    dg_next_tx: NextDatagramsSender,
    uni_next_tx: NextIncomingUniStreamsSender,
) {
    loop {}
}

pub struct Connection;

pub type UdpSessionMap = Mutex<HashMap<u32, MpscSender<(Bytes, Address)>>>;
