use super::Address;

// +----------+
// |   ADDR   |
// +----------+
// | Variable |
// +----------+
#[derive(Clone, Debug)]
pub struct Connect {
    pub addr: Address,
}

impl Connect {
    const CMD_TYPE: u8 = 0x01;

    pub fn new(addr: Address) -> Self {
        Self { addr }
    }

    pub const fn cmd_type() -> u8 {
        Self::CMD_TYPE
    }

    pub fn len(&self) -> usize {
        self.addr.len()
    }
}
