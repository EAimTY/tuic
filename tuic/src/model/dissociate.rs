use super::side::{self, Side};
use crate::protocol::{Dissociate as DissociateHeader, Header};

pub struct Dissociate<M> {
    inner: Side<Tx, Rx>,
    _marker: M,
}

pub struct Tx {
    header: Header,
}

impl Dissociate<side::Tx> {
    pub(super) fn new(assoc_id: u16) -> Self {
        Self {
            inner: Side::Tx(Tx {
                header: Header::Dissociate(DissociateHeader::new(assoc_id)),
            }),
            _marker: side::Tx,
        }
    }

    pub fn header(&self) -> &Header {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        &tx.header
    }
}

pub struct Rx {
    assoc_id: u16,
}

impl Dissociate<side::Rx> {
    pub(super) fn new(assoc_id: u16) -> Self {
        Self {
            inner: Side::Rx(Rx { assoc_id }),
            _marker: side::Rx,
        }
    }

    pub fn assoc_id(&self) -> u16 {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.assoc_id
    }
}
