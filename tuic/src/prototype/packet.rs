use crate::protocol::{Address, Header, Packet as PacketHeader};

pub struct Packet<'a> {
    assoc_id: u16,
    pkt_id: u16,
    addr: Address,
    payload: &'a [u8],
    max_pkt_size: usize,
    frag_total: u8,
    next_frag_id: u8,
    next_frag_start: usize,
}

impl<'a> Packet<'a> {
    pub(super) fn new(
        assoc_id: u16,
        pkt_id: u16,
        addr: Address,
        payload: &'a [u8],
        max_pkt_size: usize,
    ) -> Self {
        let first_frag_size = max_pkt_size - PacketHeader::len_without_addr() - addr.len();
        let frag_size_addr_none =
            max_pkt_size - PacketHeader::len_without_addr() - Address::None.len();

        let frag_total = if first_frag_size < payload.len() {
            (1 + (payload.len() - first_frag_size) / frag_size_addr_none + 1) as u8
        } else {
            1u8
        };

        Self {
            assoc_id,
            pkt_id,
            addr,
            payload,
            max_pkt_size,
            frag_total,
            next_frag_id: 0,
            next_frag_start: 0,
        }
    }
}

impl<'a> Iterator for Packet<'a> {
    type Item = (Header, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_frag_id < self.frag_total {
            let payload_size =
                self.max_pkt_size - PacketHeader::len_without_addr() - self.addr.len();
            let next_frag_end = (self.next_frag_start + payload_size).min(self.payload.len());

            let header = Header::Packet(PacketHeader::new(
                self.assoc_id,
                self.pkt_id,
                self.frag_total,
                self.next_frag_id,
                (next_frag_end - self.next_frag_start) as u16,
                self.addr.take(),
            ));

            let payload = &self.payload[self.next_frag_start..next_frag_end];

            self.next_frag_id += 1;
            self.next_frag_start = next_frag_end;

            Some((header, payload))
        } else {
            None
        }
    }
}

impl ExactSizeIterator for Packet<'_> {
    fn len(&self) -> usize {
        self.frag_total as usize
    }
}
