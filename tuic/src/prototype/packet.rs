use crate::protocol::Address;

pub struct Packet<'a> {
    assoc_id: u16,
    addr: Address,
    payload: &'a [u8],
    frag_len: usize,
}

impl<'a> Packet<'a> {
    pub(super) fn new(assoc_id: u16, addr: Address, payload: &'a [u8], frag_len: usize) -> Self {
        Self {
            assoc_id,
            addr,
            payload,
            frag_len,
        }
    }
}
