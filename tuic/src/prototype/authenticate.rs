use super::{
    side::{self, Side, SideMarker},
    TaskRegister,
};
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
    _task_reg: TaskRegister,
}

pub struct Rx;

impl Authenticate<side::Tx> {
    pub(super) fn new(task_reg: TaskRegister, token: [u8; 8]) -> Self {
        Self {
            inner: Side::Tx(Tx {
                header: Header::Authenticate(AuthenticateHeader::new(token)),
                _task_reg: task_reg,
            }),
            _marker: side::Tx,
        }
    }

    pub fn header(&self) -> &Header {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        &tx.header
    }
}
