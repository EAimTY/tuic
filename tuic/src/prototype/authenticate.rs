use super::side::{self, Side, SideMarker};
use crate::protocol::{Authenticate as AuthenticateHeader, Header};

pub struct Authenticate<M>
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

impl Authenticate<side::Tx> {
    pub(super) fn new(token: [u8; 8]) -> Self {
        Self {
            inner: Side::Tx(Tx {
                header: Header::Authenticate(AuthenticateHeader::new(token)),
            }),
            _marker: side::Tx,
        }
    }

    pub fn header(&self) -> &Header {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        &tx.header
    }
}
