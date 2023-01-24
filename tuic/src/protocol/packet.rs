use super::{Address, Command};

// +----------+--------+------------+---------+------+----------+
// | ASSOC_ID | PKT_ID | FRAG_TOTAL | FRAG_ID | SIZE |   ADDR   |
// +----------+--------+------------+---------+------+----------+
// |    2     |   2    |     1      |    1    |  2   | Variable |
// +----------+--------+------------+---------+------+----------+
#[derive(Clone, Debug)]
pub struct Packet {
    assoc_id: u16,
    pkt_id: u16,
    frag_total: u8,
    frag_id: u8,
    size: u16,
    addr: Address,
}

impl Packet {
    pub(super) const TYPE_CODE: u8 = 0x02;

    pub const fn new(
        assoc_id: u16,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        size: u16,
        addr: Address,
    ) -> Self {
        Self {
            assoc_id,
            pkt_id,
            frag_total,
            frag_id,
            size,
            addr,
        }
    }

    pub fn assoc_id(&self) -> &u16 {
        &self.assoc_id
    }

    pub fn pkt_id(&self) -> &u16 {
        &self.pkt_id
    }

    pub fn frag_total(&self) -> &u8 {
        &self.frag_total
    }

    pub fn frag_id(&self) -> &u8 {
        &self.frag_id
    }

    pub fn size(&self) -> &u16 {
        &self.size
    }

    pub fn addr(&self) -> &Address {
        &self.addr
    }

    pub const fn len_without_addr() -> usize {
        2 + 2 + 1 + 1 + 2
    }
}

impl Command for Packet {
    fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    fn len(&self) -> usize {
        2 + 2 + 1 + 1 + 2 + self.addr.len()
    }
}

impl From<Packet> for (u16, u16, u8, u8, u16, Address) {
    fn from(pkt: Packet) -> Self {
        (
            pkt.assoc_id,
            pkt.pkt_id,
            pkt.frag_total,
            pkt.frag_id,
            pkt.size,
            pkt.addr,
        )
    }
}
