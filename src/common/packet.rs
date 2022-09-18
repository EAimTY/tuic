use crate::{
    protocol::{Address, Command},
    RecvStream,
};
use bytes::{Bytes, BytesMut};
use parking_lot::Mutex;
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;

pub struct NeedAccept;
pub struct NeedAssembly;
pub struct Ready;

pub struct Packet<S> {
    assoc_id: u32,
    pkt_id: u16,
    frag_id: Option<u8>,
    frag_total: u8,
    len: u16,
    addr: Option<Address>,
    src: Option<RecvStream>,
    pkt_buf: Option<Arc<PacketBuffer>>,
    inner: Option<Bytes>,
    _state: S,
}

impl Packet<NeedAccept> {
    pub(super) fn new(
        assoc_id: u32,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        len: u16,
        addr: Option<Address>,
        stream: RecvStream,
    ) -> Self {
        Self {
            assoc_id,
            pkt_id,
            frag_id: Some(frag_id),
            frag_total,
            len,
            addr,
            src: Some(stream),
            pkt_buf: None,
            inner: None,
            _state: NeedAccept,
        }
    }

    pub async fn accept(self) -> Result<Packet<Ready>, PacketError> {
        todo!()
    }
}

impl Packet<NeedAssembly> {
    pub(super) fn new(
        assoc_id: u32,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        len: u16,
        addr: Option<Address>,
        pkt_buf: Arc<PacketBuffer>,
        pkt: Bytes,
    ) -> Self {
        Self {
            assoc_id,
            pkt_id,
            frag_id: Some(frag_id),
            frag_total,
            len,
            addr,
            src: None,
            pkt_buf: Some(pkt_buf),
            inner: Some(pkt),
            _state: NeedAssembly,
        }
    }

    pub fn assemble(self) -> Result<Packet<Ready>, PacketError> {
        todo!()
    }
}

impl Packet<Ready> {
    fn new(
        assoc_id: u32,
        pkt_id: u16,
        frag_total: u8,
        len: u16,
        addr: Address,
        pkt: Bytes,
    ) -> Self {
        Self {
            assoc_id,
            pkt_id,
            frag_id: None,
            frag_total,
            len,
            addr: Some(addr),
            src: None,
            pkt_buf: None,
            inner: Some(pkt),
            _state: Ready,
        }
    }
}

pub(crate) struct PacketBuffer(Mutex<HashMap<PacketBufferKey, PacketBufferValue>>);

impl PacketBuffer {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    pub(crate) fn insert(
        &mut self,
        pkt: Packet<NeedAssembly>,
    ) -> Result<Option<Packet<Ready>>, PacketError> {
        let mut pkt_buf = self.0.lock();
        let key = PacketBufferKey {
            assoc_id: pkt.assoc_id,
            pkt_id: pkt.pkt_id,
        };

        if pkt.frag_id.unwrap() == 0 && pkt.addr.is_none() {
            pkt_buf.remove(&key);
            return Err(PacketError::NoAddress);
        }

        if pkt.frag_id.unwrap() != 0 && pkt.addr.is_some() {
            pkt_buf.remove(&key);
            return Err(PacketError::UnexpectedAddress);
        }

        match pkt_buf.entry(key) {
            Entry::Occupied(mut entry) => {
                let v = entry.get_mut();

                if pkt.frag_total == 0
                    || pkt.frag_id.unwrap() >= pkt.frag_total
                    || v.buf.len() != pkt.frag_total as usize
                    || v.buf[pkt.frag_id.unwrap() as usize].is_some()
                {
                    return Err(PacketError::BadFragment);
                }

                v.total_len += pkt.len as usize;
                v.buf[pkt.frag_id.unwrap() as usize] = Some(pkt.inner.unwrap());
                v.recv_count += 1;

                if v.recv_count == pkt.frag_total as usize {
                    let v = entry.remove();
                    let mut res = BytesMut::with_capacity(v.total_len);

                    for pkt in v.buf {
                        res.extend_from_slice(&pkt.unwrap());
                    }

                    Ok(Some(Packet::<Ready>::new(
                        pkt.assoc_id,
                        pkt.pkt_id,
                        pkt.frag_total,
                        pkt.len,
                        v.addr.unwrap(),
                        res.freeze(),
                    )))
                } else {
                    Ok(None)
                }
            }
            Entry::Vacant(entry) => {
                if pkt.frag_total == 0 || pkt.frag_id.unwrap() >= pkt.frag_total {
                    return Err(PacketError::BadFragment);
                }

                if pkt.frag_total == 1 {
                    return Ok(Some(Packet::<Ready>::new(
                        pkt.assoc_id,
                        pkt.pkt_id,
                        pkt.frag_total,
                        pkt.len,
                        pkt.addr.unwrap(),
                        pkt.inner.unwrap(),
                    )));
                }

                let mut v = PacketBufferValue {
                    buf: vec![None; pkt.frag_total as usize],
                    addr: pkt.addr,
                    recv_count: 0,
                    total_len: 0,
                    c_time: Instant::now(),
                };

                v.total_len += pkt.len as usize;
                v.buf[pkt.frag_id.unwrap() as usize] = Some(pkt.inner.unwrap());
                v.recv_count += 1;
                entry.insert(v);

                Ok(None)
            }
        }
    }

    fn collect_garbage(&self, timeout: Duration) {
        self.0.lock().retain(|_, v| v.c_time.elapsed() < timeout);
    }

    pub(crate) fn get_handler(self: Arc<Self>) -> PacketBufferHandle {
        PacketBufferHandle(self.clone())
    }
}

pub struct PacketBufferHandle(Arc<PacketBuffer>);

impl PacketBufferHandle {
    fn new(pkt_buf: Arc<PacketBuffer>) -> Self {
        Self(pkt_buf)
    }
    pub fn collect_garbage(&self, timeout: Duration) {
        self.0.collect_garbage(timeout)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PacketBufferKey {
    assoc_id: u32,
    pkt_id: u16,
}

#[derive(Debug)]
struct PacketBufferValue {
    buf: Vec<Option<Bytes>>,
    addr: Option<Address>,
    recv_count: usize,
    total_len: usize,
    c_time: Instant,
}

#[derive(Error, Debug)]
pub enum PacketError {
    #[error("missing address in packet with frag_id 0")]
    NoAddress,
    #[error("unexpected address in packet")]
    UnexpectedAddress,
    #[error("received bad-fragmented packet")]
    BadFragment,
}

#[inline]
pub(crate) fn split_packet(pkt: Bytes, addr: &Address, max_datagram_size: usize) -> SplitPacket {
    SplitPacket::new(pkt, addr, max_datagram_size)
}

#[derive(Debug)]
pub(crate) struct SplitPacket {
    pkt: Bytes,
    max_pkt_size: usize,
    next_start: usize,
    next_end: usize,
    len: usize,
}

impl SplitPacket {
    #[inline]
    fn new(pkt: Bytes, addr: &Address, max_datagram_size: usize) -> Self {
        const DEFAULT_HEADER: Command = Command::Packet {
            assoc_id: 0,
            pkt_id: 0,
            frag_total: 0,
            frag_id: 0,
            len: 0,
            addr: None,
        };

        let first_pkt_size =
            max_datagram_size - DEFAULT_HEADER.serialized_len() - addr.serialized_len();
        let max_pkt_size = max_datagram_size - DEFAULT_HEADER.serialized_len();
        let len = if first_pkt_size > pkt.len() {
            1 + (pkt.len() - first_pkt_size) / max_pkt_size + 1
        } else {
            1
        };

        Self {
            pkt,
            max_pkt_size,
            next_start: 0,
            next_end: first_pkt_size,
            len,
        }
    }
}

impl Iterator for SplitPacket {
    type Item = Bytes;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_start <= self.pkt.len() {
            let next = self
                .pkt
                .slice(self.next_start..self.next_end.min(self.pkt.len()));

            self.next_start += self.max_pkt_size;
            self.next_end += self.max_pkt_size;

            Some(next)
        } else {
            None
        }
    }
}

impl ExactSizeIterator for SplitPacket {
    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}
