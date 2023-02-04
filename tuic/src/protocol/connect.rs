use super::Address;

// +----------+
// |   ADDR   |
// +----------+
// | Variable |
// +----------+
#[derive(Clone, Debug)]
pub struct Connect {
    addr: Address,
}

impl Connect {
    const TYPE_CODE: u8 = 0x01;

    pub const fn new(addr: Address) -> Self {
        Self { addr }
    }

    pub fn addr(&self) -> &Address {
        &self.addr
    }

    pub const fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.addr.len()
    }
}

impl From<Connect> for (Address,) {
    fn from(conn: Connect) -> Self {
        (conn.addr,)
    }
}
