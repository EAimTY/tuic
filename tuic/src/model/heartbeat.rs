use super::side::{self, Side};
use crate::protocol::{Header, Heartbeat as HeartbeatHeader};

pub struct Heartbeat<M> {
    inner: Side<Tx, Rx>,
    _marker: M,
}

struct Tx {
    header: Header,
}

impl Heartbeat<side::Tx> {
    pub(super) fn new() -> Self {
        Self {
            inner: Side::Tx(Tx {
                header: Header::Heartbeat(HeartbeatHeader::new()),
            }),
            _marker: side::Tx,
        }
    }

    /// Returns the header of the `Heartbeat` command
    pub fn header(&self) -> &Header {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        &tx.header
    }
}

struct Rx;

impl Heartbeat<side::Rx> {
    pub(super) fn new() -> Self {
        Self {
            inner: Side::Rx(Rx),
            _marker: side::Rx,
        }
    }
}
