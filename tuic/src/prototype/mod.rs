use crate::protocol::Address;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, Weak,
    },
};

mod authenticate;
mod connect;
mod dissociate;
mod heartbeat;
mod packet;

pub use self::{
    authenticate::Authenticate,
    connect::Connect,
    dissociate::Dissociate,
    heartbeat::Heartbeat,
    packet::{Fragment, Packet},
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

    pub fn send_authenticate(&self, token: [u8; 8]) -> Authenticate<side::Tx> {
        Authenticate::new(self.local_active_task_count.reg(), token)
    }

    pub fn send_connect(&self, addr: Address) -> Connect<side::Tx> {
        Connect::<side::Tx>::new(self.local_active_task_count.reg(), addr)
    }

    pub fn send_packet(
        &self,
        assoc_id: u16,
        addr: Address,
        max_pkt_size: usize,
    ) -> Packet<side::Tx> {
        self.udp_sessions.lock().send(assoc_id, addr, max_pkt_size)
    }

    pub fn send_dissociate(&self, assoc_id: u16) -> Dissociate<side::Tx> {
        self.udp_sessions.lock().dissociate(assoc_id)
    }

    pub fn send_heartbeat(&self) -> Heartbeat<side::Tx> {
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

    fn send<'a>(&mut self, assoc_id: u16, addr: Address, max_pkt_size: usize) -> Packet<side::Tx> {
        self.sessions
            .entry(assoc_id)
            .or_insert_with(|| UdpSession::new(self.local_active_task_count.reg()))
            .send(assoc_id, addr, max_pkt_size)
    }

    fn dissociate(&mut self, assoc_id: u16) -> Dissociate<side::Tx> {
        self.sessions.remove(&assoc_id);
        Dissociate::new(assoc_id)
    }
}

struct UdpSession {
    next_pkt_id: AtomicU16,
    _task_reg: TaskRegister,
}

impl UdpSession {
    fn new(task_reg: TaskRegister) -> Self {
        Self {
            next_pkt_id: AtomicU16::new(0),
            _task_reg: task_reg,
        }
    }

    fn send<'a>(&self, assoc_id: u16, addr: Address, max_pkt_size: usize) -> Packet<side::Tx> {
        Packet::new(
            assoc_id,
            self.next_pkt_id.fetch_add(1, Ordering::AcqRel),
            addr,
            max_pkt_size,
        )
    }
}

pub mod side {
    pub struct Tx;
    pub struct Rx;

    pub trait SideMarker {}
    impl SideMarker for Tx {}
    impl SideMarker for Rx {}

    pub(super) enum Side<T, R> {
        Tx(T),
        Rx(R),
    }
}
