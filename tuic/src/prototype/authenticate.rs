use super::TaskRegister;
use crate::protocol::{Authenticate as AuthenticateHeader, Header};

pub struct Authenticate {
    header: Header,
    _task_reg: TaskRegister,
}

impl Authenticate {
    pub(super) fn new(task_reg: TaskRegister, token: [u8; 8]) -> Self {
        Self {
            header: Header::Authenticate(AuthenticateHeader::new(token)),
            _task_reg: task_reg,
        }
    }
}
