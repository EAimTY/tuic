use crate::protocol::Address;
use parking_lot::Mutex;
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{Arc, Weak},
};

mod authenticate;
mod connect;
mod dissociate;
mod heartbeat;
mod packet;

pub use self::{
    authenticate::Authenticate, connect::Connect, dissociate::Dissociate, heartbeat::Heartbeat,
    packet::Packet,
};

pub struct Connection {
    udp_sessions: Mutex<UdpSessions>,
    local_active_task_count: ActiveTaskCount,
}

impl Connection {
    pub fn new() -> Self {
        let local_active_task_count = ActiveTaskCount::new();

        Self {
            udp_sessions: Mutex::new(UdpSessions::new(local_active_task_count.clone())),
            local_active_task_count,
        }
    }

    pub fn authenticate(&self, token: [u8; 8]) -> Authenticate {
        Authenticate::new(self.local_active_task_count.reg(), token)
    }

    pub fn connect(&self, addr: Address) -> Connect {
        Connect::new(self.local_active_task_count.reg(), addr)
    }

    pub fn packet<'a>(
        &self,
        assoc_id: u16,
        addr: Address,
        payload: &'a [u8],
        frag_len: usize,
    ) -> Packet<'a> {
        self.udp_sessions
            .lock()
            .send(assoc_id, addr, payload, frag_len)
    }

    pub fn dissociate(&self, assoc_id: u16) -> Dissociate {
        self.udp_sessions.lock().dissociate(assoc_id)
    }

    pub fn heartbeat(&self) -> Heartbeat {
        Heartbeat::new()
    }

    pub fn local_active_task_count(&self) -> usize {
        self.local_active_task_count.get()
    }
}

#[derive(Clone)]
struct ActiveTaskCount(Arc<()>);
struct TaskRegister(Weak<()>);

impl ActiveTaskCount {
    fn new() -> Self {
        Self(Arc::new(()))
    }

    fn reg(&self) -> TaskRegister {
        TaskRegister(Arc::downgrade(&self.0))
    }

    fn get(&self) -> usize {
        Arc::weak_count(&self.0)
    }
}

struct UdpSessions {
    sessions: HashMap<u16, UdpSession>,
    local_active_task_count: ActiveTaskCount,
}

impl UdpSessions {
    fn new(local_active_task_count: ActiveTaskCount) -> Self {
        Self {
            sessions: HashMap::new(),
            local_active_task_count,
        }
    }

    fn send<'a>(
        &mut self,
        assoc_id: u16,
        addr: Address,
        payload: &'a [u8],
        frag_len: usize,
    ) -> Packet<'a> {
        match self.sessions.entry(assoc_id) {
            Entry::Occupied(_) => {}
            Entry::Vacant(entry) => {
                entry.insert(UdpSession::new(self.local_active_task_count.reg()));
            }
        }

        Packet::new(assoc_id, addr, payload, frag_len)
    }

    fn dissociate(&mut self, assoc_id: u16) -> Dissociate {
        self.sessions.remove(&assoc_id);
        Dissociate::new(self.local_active_task_count.reg(), assoc_id)
    }
}

struct UdpSession {
    _task_reg: TaskRegister,
}

impl UdpSession {
    fn new(task_reg: TaskRegister) -> Self {
        Self {
            _task_reg: task_reg,
        }
    }
}
