use super::TaskRegister;
use crate::protocol::{Dissociate as DissociateHeader, Header};

pub struct Dissociate {
    header: Header,
    _task_reg: TaskRegister,
}

impl Dissociate {
    pub(super) fn new(task_reg: TaskRegister, assoc_id: u16) -> Self {
        Self {
            header: Header::Dissociate(DissociateHeader::new(assoc_id)),
            _task_reg: task_reg,
        }
    }
}
