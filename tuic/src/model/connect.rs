use super::side::{self, Side};
use crate::{Address, Connect as ConnectHeader, Header};
use register_count::Register;
use std::fmt::{Debug, Formatter, Result as FmtResult};

/// The model of the `Connect` command
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

    /// Returns the header of the `Connect` command
    pub fn header(&self) -> &Header {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        &tx.header
    }
}

impl Debug for Connect<side::Tx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let Side::Tx(tx) = &self.inner else { unreachable!() };
        f.debug_struct("Connect")
            .field("header", &tx.header)
            .finish()
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

    /// Returns the address
    pub fn addr(&self) -> &Address {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        &rx.addr
    }
}

impl Debug for Connect<side::Rx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let Side::Rx(rx) = &self.inner else { unreachable!() };
        f.debug_struct("Connect").field("addr", &rx.addr).finish()
    }
}
