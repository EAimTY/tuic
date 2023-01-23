use super::Address;

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
    pub addr: Option<Address>,
}

impl Packet {
    const CMD_TYPE: u8 = 0x02;

    pub fn new(
        assoc_id: u16,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        len: u16,
        addr: Option<Address>,
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

    pub const fn cmd_type() -> u8 {
        Self::CMD_TYPE
    }

    pub fn len(&self) -> usize {
        2 + 2 + 1 + 1 + 2 + self.addr.as_ref().map_or(0, |addr| addr.len())
    }
}
