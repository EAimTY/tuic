use super::{Address, Connection};
use bytes::Bytes;
use parking_lot::Mutex;
use quinn::{Datagrams, IncomingUniStreams};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{Receiver as MpscReceiver, Sender as MpscSender};

impl Connection {
    pub fn handle_associate(
        &self,
        assoc_id: u32,
        pkt_send_rx: MpscReceiver<(Bytes, Address)>,
        pkt_receive_tx: MpscSender<(Bytes, Address)>,
    ) {
        self.assoc_map.insert(assoc_id, pkt_receive_tx);
    }

    pub async fn listen_incoming(
        uni_streams: IncomingUniStreams,
        datagrams: Datagrams,
        assoc_map: Arc<AssociateMap>,
    ) {
    }
}

pub struct AssociateMap(Mutex<HashMap<u32, MpscSender<(Bytes, Address)>>>);

impl AssociateMap {
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    fn insert(&self, assoc_id: u32, pkt_receive_tx: MpscSender<(Bytes, Address)>) {
        self.0.lock().insert(assoc_id, pkt_receive_tx);
    }
}
