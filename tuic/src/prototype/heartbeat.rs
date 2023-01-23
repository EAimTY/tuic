use crate::protocol::{Header, Heartbeat as HeartbeatHeader};

pub struct Heartbeat {
    header: Header,
}

impl Heartbeat {
    pub(super) fn new() -> Self {
        Self {
            header: Header::Heartbeat(HeartbeatHeader::new()),
        }
    }
}
