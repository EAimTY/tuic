use super::side::{self, Side};
use crate::{Dissociate as DissociateHeader, Header};
use std::fmt::{Debug, Formatter, Result as FmtResult};

/// The model of the `Dissociate` command
pub struct Dissociate<M> {
    inner: Side<Tx, Rx>,
    _marker: M,
}

struct Tx {
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

    /// Returns the header of the `Dissociate` command
    pub fn header(&self) -> &Header {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        &tx.header
    }
}

impl Debug for Dissociate<side::Tx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        f.debug_struct("Dissociate")
            .field("header", &tx.header)
            .finish()
    }
}

struct Rx {
    assoc_id: u16,
}

impl Dissociate<side::Rx> {
    pub(super) fn new(assoc_id: u16) -> Self {
        Self {
            inner: Side::Rx(Rx { assoc_id }),
            _marker: side::Rx,
        }
    }

    /// Returns the UDP session ID
    pub fn assoc_id(&self) -> u16 {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        rx.assoc_id
    }
}

impl Debug for Dissociate<side::Rx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        f.debug_struct("Dissociate")
            .field("assoc_id", &rx.assoc_id)
            .finish()
    }
}
