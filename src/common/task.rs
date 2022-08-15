use super::stream::{RecvStream, Stream};
use crate::protocol::{Address, Command};
use bytes::Bytes;

#[derive(Clone, Debug)]
pub struct Packet {
    pub id: u16,
    pub associate_id: u32,
    pub address: Address,
    pub data: Bytes,
}

impl Packet {
    pub(crate) fn new(assoc_id: u32, pkt_id: u16, addr: Address, pkt: Bytes) -> Self {
        Self {
            id: pkt_id,
            associate_id: assoc_id,
            address: addr,
            data: pkt,
        }
    }
}

pub(crate) struct RawTask {
    header: Command,
    payload: RawTaskPayload,
}

impl RawTask {
    pub(crate) fn new(header: Command, payload: RawTaskPayload) -> Self {
        Self { header, payload }
    }
}

pub(crate) enum RawTaskPayload {
    BiStream(Stream),
    UniStream(RecvStream),
    Datagram(Bytes),
}
