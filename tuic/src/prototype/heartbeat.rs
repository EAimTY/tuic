use super::side::{self, Side};
use crate::protocol::{Header, Heartbeat as HeartbeatHeader};

pub struct Heartbeat<M> {
    inner: Side<Tx, Rx>,
    _marker: M,
}

pub struct Tx {
    header: Header,
}

pub struct Rx;

impl Heartbeat<side::Tx> {
    pub(super) fn new() -> Self {
        Self {
            inner: Side::Tx(Tx {
                header: Header::Heartbeat(HeartbeatHeader::new()),
            }),
            _marker: side::Tx,
        }
    }

    pub fn header(&self) -> &Header {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        &tx.header
    }
}
