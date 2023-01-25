use crate::protocol::{Address, Connect as ConnectHeader, Packet as PacketHeader};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, Weak,
    },
    time::{Duration, Instant},
};
use thiserror::Error;

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

pub struct Connection<B> {
    udp_sessions: Arc<Mutex<UdpSessions<B>>>,
    task_connect_count: TaskCount,
    task_associate_count: TaskCount,
}

impl<B> Connection<B>
where
    B: AsRef<[u8]>,
{
    pub fn new() -> Self {
        let task_associate_count = TaskCount::new();

        Self {
            udp_sessions: Arc::new(Mutex::new(UdpSessions::new(task_associate_count.clone()))),
            task_connect_count: TaskCount::new(),
            task_associate_count,
        }
    }

    pub fn send_authenticate(&self, token: [u8; 8]) -> Authenticate<side::Tx> {
        Authenticate::new(token)
    }

    pub fn send_connect(&self, addr: Address) -> Connect<side::Tx> {
        Connect::<side::Tx>::new(self.task_connect_count.register(), addr)
    }

    pub fn recv_connect(&self, header: ConnectHeader) -> Connect<side::Rx> {
        let (addr,) = header.into();
        Connect::<side::Rx>::new(self.task_connect_count.register(), addr)
    }

    pub fn send_packet(
        &self,
        assoc_id: u16,
        addr: Address,
        max_pkt_size: usize,
    ) -> Packet<side::Tx, B> {
        self.udp_sessions
            .lock()
            .send_packet(assoc_id, addr, max_pkt_size)
    }

    pub fn recv_packet(&self, header: PacketHeader) -> Packet<side::Rx, B> {
        let (assoc_id, pkt_id, frag_total, frag_id, size, addr) = header.into();
        self.udp_sessions.lock().recv_packet(
            self.udp_sessions.clone(),
            assoc_id,
            pkt_id,
            frag_total,
            frag_id,
            size,
            addr,
        )
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

    pub fn collect_garbage(&self, timeout: Duration) {
        self.udp_sessions.lock().collect_garbage(timeout);
    }
}

#[derive(Clone)]
struct TaskCount(Arc<()>);
struct TaskRegister(Weak<()>);

impl TaskCount {
    fn new() -> Self {
        Self(Arc::new(()))
    }

    fn register(&self) -> TaskRegister {
        TaskRegister(Arc::downgrade(&self.0))
    }

    fn get(&self) -> usize {
        Arc::weak_count(&self.0)
    }
}

pub mod side {
    pub struct Tx;
    pub struct Rx;

    pub(super) enum Side<T, R> {
        Tx(T),
        Rx(R),
    }
}

struct UdpSessions<B> {
    sessions: HashMap<u16, UdpSession<B>>,
    task_associate_count: TaskCount,
}

impl<B> UdpSessions<B>
where
    B: AsRef<[u8]>,
{
    fn new(task_associate_count: TaskCount) -> Self {
        Self {
            sessions: HashMap::new(),
            task_associate_count,
        }
    }

    fn send_packet<'a>(
        &mut self,
        assoc_id: u16,
        addr: Address,
        max_pkt_size: usize,
    ) -> Packet<side::Tx, B> {
        self.sessions
            .entry(assoc_id)
            .or_insert_with(|| UdpSession::new(self.task_associate_count.register()))
            .send_packet(assoc_id, addr, max_pkt_size)
    }

    fn recv_packet<'a>(
        &mut self,
        sessions: Arc<Mutex<Self>>,
        assoc_id: u16,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        size: u16,
        addr: Address,
    ) -> Packet<side::Rx, B> {
        self.sessions
            .entry(assoc_id)
            .or_insert_with(|| UdpSession::new(self.task_associate_count.register()))
            .recv_packet(sessions, assoc_id, pkt_id, frag_total, frag_id, size, addr)
    }

    fn dissociate(&mut self, assoc_id: u16) -> Dissociate<side::Tx> {
        self.sessions.remove(&assoc_id);
        Dissociate::new(assoc_id)
    }

    fn insert<A>(
        &mut self,
        assoc_id: u16,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        size: u16,
        addr: Address,
        data: B,
    ) -> Result<Option<(A, Address)>, AssembleError>
    where
        A: Assembled<B>,
    {
        self.sessions
            .entry(assoc_id)
            .or_insert_with(|| UdpSession::new(self.task_associate_count.register()))
            .insert(pkt_id, frag_total, frag_id, size, addr, data)
    }

    fn collect_garbage(&mut self, timeout: Duration) {
        for (_, session) in self.sessions.iter_mut() {
            session.collect_garbage(timeout);
        }
    }
}

struct UdpSession<B> {
    pkt_buf: HashMap<u16, PacketBuffer<B>>,
    next_pkt_id: AtomicU16,
    _task_reg: TaskRegister,
}

impl<B> UdpSession<B>
where
    B: AsRef<[u8]>,
{
    fn new(task_reg: TaskRegister) -> Self {
        Self {
            pkt_buf: HashMap::new(),
            next_pkt_id: AtomicU16::new(0),
            _task_reg: task_reg,
        }
    }

    fn send_packet(
        &self,
        assoc_id: u16,
        addr: Address,
        max_pkt_size: usize,
    ) -> Packet<side::Tx, B> {
        Packet::<side::Tx, B>::new(
            assoc_id,
            self.next_pkt_id.fetch_add(1, Ordering::AcqRel),
            addr,
            max_pkt_size,
        )
    }

    fn recv_packet(
        &self,
        sessions: Arc<Mutex<UdpSessions<B>>>,
        assoc_id: u16,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        size: u16,
        addr: Address,
    ) -> Packet<side::Rx, B> {
        Packet::<side::Rx, B>::new(sessions, assoc_id, pkt_id, frag_total, frag_id, size, addr)
    }

    fn insert<A>(
        &mut self,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        size: u16,
        addr: Address,
        data: B,
    ) -> Result<Option<(A, Address)>, AssembleError>
    where
        A: Assembled<B>,
    {
        let res = self
            .pkt_buf
            .entry(pkt_id)
            .or_insert_with(|| PacketBuffer::new(frag_total))
            .insert(frag_total, frag_id, size, addr, data)?;

        if res.is_some() {
            self.pkt_buf.remove(&pkt_id);
        }

        Ok(res)
    }

    fn collect_garbage(&mut self, timeout: Duration) {
        self.pkt_buf.retain(|_, buf| buf.c_time.elapsed() < timeout);
    }
}

struct PacketBuffer<B> {
    buf: Vec<Option<B>>,
    frag_total: u8,
    frag_received: u8,
    addr: Address,
    c_time: Instant,
}

impl<B> PacketBuffer<B>
where
    B: AsRef<[u8]>,
{
    fn new(frag_total: u8) -> Self {
        let mut buf = Vec::with_capacity(frag_total as usize);
        buf.resize_with(frag_total as usize, || None);

        Self {
            buf,
            frag_total,
            frag_received: 0,
            addr: Address::None,
            c_time: Instant::now(),
        }
    }

    fn insert<A>(
        &mut self,
        frag_total: u8,
        frag_id: u8,
        size: u16,
        addr: Address,
        data: B,
    ) -> Result<Option<(A, Address)>, AssembleError>
    where
        A: Assembled<B>,
    {
        if data.as_ref().len() != size as usize {
            return Err(AssembleError::InvalidFragmentSize);
        }

        if frag_id >= frag_total {
            return Err(AssembleError::InvalidFragmentId);
        }

        if (frag_id == 0 && addr.is_none()) || (frag_id != 0 && !addr.is_none()) {
            return Err(AssembleError::InvalidAddress);
        }

        if self.buf[frag_id as usize].is_some() {
            return Err(AssembleError::DuplicateFragment);
        }

        self.buf[frag_id as usize] = Some(data);
        self.frag_received += 1;

        if frag_id == 0 {
            self.addr = addr;
        }

        if self.frag_received == self.frag_total {
            let iter = self.buf.iter_mut().map(|x| x.take().unwrap());
            Ok(Some((A::assemble(iter)?, self.addr.take())))
        } else {
            Ok(None)
        }
    }
}

pub trait Assembled<B>
where
    Self: Sized,
    B: AsRef<[u8]>,
{
    fn assemble(buf: impl IntoIterator<Item = B>) -> Result<Self, AssembleError>;
}

#[derive(Debug, Error)]
pub enum AssembleError {
    #[error("invalid fragment size")]
    InvalidFragmentSize,
    #[error("invalid fragment id")]
    InvalidFragmentId,
    #[error("invalid address")]
    InvalidAddress,
    #[error("duplicate fragment")]
    DuplicateFragment,
}
