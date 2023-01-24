use super::{
    side::{self, Side, SideMarker},
    TaskRegister,
};
use crate::protocol::{Address, Connect as ConnectHeader, Header};

pub struct Connect<M>
where
    M: SideMarker,
{
    inner: Side<Tx, Rx>,
    _marker: M,
}

struct Tx {
    header: Header,
    _task_reg: TaskRegister,
}

impl Connect<side::Tx> {
    pub(super) fn new(task_reg: TaskRegister, addr: Address) -> Self {
        Self {
            inner: Side::Tx(Tx {
                header: Header::Connect(ConnectHeader::new(addr)),
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

struct Rx {
    addr: Address,
}

impl Connect<side::Rx> {
    pub(super) fn new(header: ConnectHeader) -> Self {
        let (addr,) = header.into();
        Self {
            inner: Side::Rx(Rx { addr }),
            _marker: side::Rx,
        }
    }
}
