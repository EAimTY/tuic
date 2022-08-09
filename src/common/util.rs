use crate::{
    protocol::{Address, Command},
    task::Packet,
};
use bytes::{Bytes, BytesMut};
use std::{
    collections::{hash_map::Entry, HashMap},
    time::{Duration, Instant},
};
use thiserror::Error;

#[derive(Debug)]
pub(crate) struct PacketBuffer(HashMap<PacketBufferKey, PacketBufferValue>);

impl PacketBuffer {
    pub(crate) fn new() -> Self {
        Self(HashMap::new())
    }

    pub(crate) fn insert(
        &mut self,
        assoc_id: u32,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        addr: Option<Address>,
        pkt: Bytes,
    ) -> Result<Option<Packet>, PacketBufferError> {
        let key = PacketBufferKey { assoc_id, pkt_id };

        if frag_id == 0 && addr.is_none() {
            self.0.remove(&key);
            return Err(PacketBufferError::NoAddress);
        }

        if frag_id != 0 && addr.is_some() {
            self.0.remove(&key);
            return Err(PacketBufferError::UnexpectedAddress);
        }

        match self.0.entry(key) {
            Entry::Occupied(mut entry) => {
                let v = entry.get_mut();

                if frag_total == 0
                    || frag_id >= frag_total
                    || v.buf.len() != frag_total as usize
                    || v.buf[frag_id as usize].is_some()
                {
                    return Err(PacketBufferError::BadFragment);
                }

                v.total_len += pkt.len();
                v.buf[frag_id as usize] = Some(pkt);
                v.recv_count += 1;

                if v.recv_count == frag_total as usize {
                    let v = entry.remove();
                    let mut res = BytesMut::with_capacity(v.total_len);

                    for pkt in v.buf {
                        res.extend_from_slice(&pkt.unwrap());
                    }

                    Ok(Some(Packet::new(
                        assoc_id,
                        pkt_id,
                        v.addr.unwrap(),
                        res.freeze(),
                    )))
                } else {
                    Ok(None)
                }
            }
            Entry::Vacant(entry) => {
                if frag_total == 0 || frag_id >= frag_total {
                    return Err(PacketBufferError::BadFragment);
                }

                if frag_total == 1 {
                    return Ok(Some(Packet::new(assoc_id, pkt_id, addr.unwrap(), pkt)));
                }

                let mut v = PacketBufferValue {
                    buf: vec![None; frag_total as usize],
                    addr,
                    recv_count: 0,
                    total_len: 0,
                    c_time: Instant::now(),
                };

                v.total_len += pkt.len();
                v.buf[frag_id as usize] = Some(pkt);
                v.recv_count += 1;
                entry.insert(v);

                Ok(None)
            }
        }
    }

    pub(crate) fn collect_garbage(&mut self, timeout: Duration) {
        self.0.retain(|_, v| v.c_time.elapsed() < timeout);
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
pub(crate) enum PacketBufferError {
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
