use super::side::{self, Side};
use crate::protocol::{Address, Connect as ConnectHeader, Header};
use register_count::Register;

pub struct Connect<M> {
    inner: Side<Tx, Rx>,
    _marker: M,
}

struct Tx {
    header: Header,
    _task_reg: Register,
}

impl Connect<side::Tx> {
    pub(super) fn new(task_reg: Register, addr: Address) -> Self {
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
    _task_reg: Register,
}

impl Connect<side::Rx> {
    pub(super) fn new(task_reg: Register, addr: Address) -> Self {
        Self {
            inner: Side::Rx(Rx {
                addr,
                _task_reg: task_reg,
            }),
            _marker: side::Rx,
        }
    }

    pub fn addr(&self) -> &Address {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        &rx.addr
    }
}
