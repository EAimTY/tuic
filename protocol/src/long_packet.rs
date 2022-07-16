//! Packet reassembly buffers for long fragmented UDP packets.

use std::collections::HashMap;

use bytes::{Bytes, BytesMut};

use crate::Address;

#[derive(Clone)]
pub struct LongPacketBuffer {
    recv_cnt: u8,
    frags: Vec<Option<Bytes>>,
    addr: Option<Address>,
}

impl LongPacketBuffer {
    pub fn new(frag_cnt: u8) -> Self {
        Self {
            recv_cnt: 0,
            frags: vec![None; frag_cnt as usize],
            addr: None,
        }
    }

    pub fn set_addr(&mut self, addr: Address) {
        self.addr = Some(addr);
    }

    pub fn on_frag(&mut self, frag_id: u8, frag: Bytes) -> Option<(Bytes, Address)> {
        if (frag_id as usize) < self.frags.len() {
            let target = &mut self.frags[frag_id as usize];
            if target.is_none() {
                self.recv_cnt += 1;
                *target = Some(frag);
                if self.recv_cnt as usize == self.frags.len() {
                    let mut packet = BytesMut::new();
                    for frag in &mut self.frags {
                        packet.extend(frag.as_ref().unwrap());
                    }
                    return Some((packet.freeze(), self.addr.as_ref().unwrap().clone()));
                }
            }
        }
        None
    }
}

pub struct LongPacketBuffers(HashMap<u32, LongPacketBuffer>);

impl LongPacketBuffers {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn on_frag(
        &mut self,
        lp_id: u32,
        frag_id: u8,
        frag_cnt: u8,
        addr: Option<Address>,
        frag: Bytes,
    ) -> Option<(Bytes, Address)> {
        let buf = self
            .0
            .entry(lp_id)
            .or_insert_with(|| LongPacketBuffer::new(frag_cnt));
        if let Some(addr) = addr {
            buf.set_addr(addr);
        }
        if let Some(result) = buf.on_frag(frag_id, frag) {
            self.0.remove(&lp_id);
            return Some(result);
        }
        None
    }
}

impl Default for LongPacketBuffers {
    fn default() -> Self {
        Self::new()
    }
}
