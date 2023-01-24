use super::side::{self, Side, SideMarker};
use crate::protocol::{Dissociate as DissociateHeader, Header};

pub struct Dissociate<M>
where
    M: SideMarker,
{
    inner: Side<Tx, Rx>,
    _marker: M,
}

pub struct Tx {
    header: Header,
}

pub struct Rx;

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
