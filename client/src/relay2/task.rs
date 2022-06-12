use super::{Address, BiStream, Connection, UdpRelayMode};
use bytes::Bytes;
use tokio::sync::oneshot::Sender as OneshotSender;

impl Connection {
    pub async fn handle_connect(self, addr: Address, tx: OneshotSender<BiStream>) {
        todo!()
    }

    pub async fn handle_packet_to(
        self,
        assoc_id: u32,
        pkt: Bytes,
        addr: Address,
        mode: UdpRelayMode<(), ()>,
    ) {
        todo!()
    }

    pub async fn handle_packet_from(self, assoc_id: u32, pkt: Bytes, addr: Address) {
        todo!()
    }

    pub async fn handle_dissociate(self, assoc_id: u32) {
        todo!()
    }
}
