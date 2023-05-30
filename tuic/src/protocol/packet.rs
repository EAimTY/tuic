use super::Address;

/// Command `Packet`
/// ```plain
/// +----------+--------+------------+---------+------+----------+
/// | ASSOC_ID | PKT_ID | FRAG_TOTAL | FRAG_ID | SIZE |   ADDR   |
/// +----------+--------+------------+---------+------+----------+
/// |    2     |   2    |     1      |    1    |  2   | Variable |
/// +----------+--------+------------+---------+------+----------+
/// ```
///
/// where:
///
/// - `ASSOC_ID` - UDP relay session ID
/// - `PKT_ID` - UDP packet ID
/// - `FRAG_TOTAL` - total number of fragments of the UDP packet
/// - `FRAG_ID` - fragment ID of the UDP packet
/// - `SIZE` - length of the (fragmented) UDP packet
/// - `ADDR` - target (from client) or source (from server) address
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
    const TYPE_CODE: u8 = 0x02;

    /// Creates a new `Packet` command
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

    /// Returns the UDP relay session ID
    pub fn assoc_id(&self) -> u16 {
        self.assoc_id
    }

    /// Returns the packet ID
    pub fn pkt_id(&self) -> u16 {
        self.pkt_id
    }

    /// Returns the total number of fragments of the UDP packet
    pub fn frag_total(&self) -> u8 {
        self.frag_total
    }

    /// Returns the fragment ID of the UDP packet
    pub fn frag_id(&self) -> u8 {
        self.frag_id
    }

    /// Returns the length of the (fragmented) UDP packet
    pub fn size(&self) -> u16 {
        self.size
    }

    /// Returns the target (from client) or source (from server) address
    pub fn addr(&self) -> &Address {
        &self.addr
    }

    /// Returns the command type code
    pub const fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    /// Returns the serialized length of the command
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
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
