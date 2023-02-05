use super::side::{self, Side};
use crate::{Header, Heartbeat as HeartbeatHeader};
use std::fmt::{Debug, Formatter, Result as FmtResult};

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

impl Debug for Heartbeat<side::Tx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        f.debug_struct("Heartbeat")
            .field("header", &tx.header)
            .finish()
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

impl Debug for Heartbeat<side::Rx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Heartbeat").finish()
    }
}
