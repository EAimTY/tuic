use super::TaskRegister;
use crate::protocol::{Address, Connect as ConnectHeader, Header};

pub struct Connect {
    header: Header,
    _task_reg: TaskRegister,
}

impl Connect {
    pub(super) fn new(task_reg: TaskRegister, addr: Address) -> Self {
        Self {
            header: Header::Connect(ConnectHeader::new(addr)),
            _task_reg: task_reg,
        }
    }

    pub fn header(&self) -> &Header {
        &self.header
    }
}
