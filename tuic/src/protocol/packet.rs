use super::{Address, Command};

// +----------+--------+------------+---------+-----+----------+
// | ASSOC_ID | PKT_ID | FRAG_TOTAL | FRAG_ID | LEN |   ADDR   |
// +----------+--------+------------+---------+-----+----------+
// |    2     |   2    |     1      |    1    |  2  | Variable |
// +----------+--------+------------+---------+-----+----------+
#[derive(Clone, Debug)]
pub struct Packet {
    pub assoc_id: u16,
    pub pkt_id: u16,
    pub frag_total: u8,
    pub frag_id: u8,
    pub len: u16,
    pub addr: Address,
}

impl Packet {
    pub(super) const TYPE_CODE: u8 = 0x02;

    pub const fn new(
        assoc_id: u16,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        len: u16,
        addr: Address,
    ) -> Self {
        Self {
            assoc_id,
            pkt_id,
            frag_total,
            frag_id,
            len,
            addr,
        }
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
