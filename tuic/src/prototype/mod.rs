use crate::protocol::{Address, Connect as ConnectHeader};
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
    task_connect_count: TaskCount,
    task_associate_count: TaskCount,
}

impl Connection {
    pub fn new() -> Self {
        let task_associate_count = TaskCount::new();

        Self {
            udp_sessions: Mutex::new(UdpSessions::new(task_associate_count.clone())),
            task_connect_count: TaskCount::new(),
            task_associate_count,
        }
    }

    pub fn send_authenticate(&self, token: [u8; 8]) -> Authenticate<side::Tx> {
        Authenticate::new(token)
    }

    pub fn send_connect(&self, addr: Address) -> Connect<side::Tx> {
        Connect::<side::Tx>::new(self.task_connect_count.reg(), addr)
    }

    pub fn recv_connect(&self, header: ConnectHeader) -> Connect<side::Rx> {
        Connect::<side::Rx>::new(self.task_connect_count.reg(), header)
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

    pub fn task_connect_count(&self) -> usize {
        self.task_connect_count.get()
    }

    pub fn task_associate_count(&self) -> usize {
        self.task_associate_count.get()
    }
}

#[derive(Clone)]
struct TaskCount(Arc<()>);
struct TaskRegister(Weak<()>);

impl TaskCount {
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
    task_associate_count: TaskCount,
}

impl UdpSessions {
    fn new(task_associate_count: TaskCount) -> Self {
        Self {
            sessions: HashMap::new(),
            task_associate_count,
        }
    }

    fn send<'a>(&mut self, assoc_id: u16, addr: Address, max_pkt_size: usize) -> Packet<side::Tx> {
        self.sessions
            .entry(assoc_id)
            .or_insert_with(|| UdpSession::new(self.task_associate_count.reg()))
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
