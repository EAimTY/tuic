use super::{Address, Command};

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
    pub(super) const TYPE_CODE: u8 = 0x01;

    pub const fn new(addr: Address) -> Self {
        Self { addr }
    }

    pub fn addr(&self) -> &Address {
        &self.addr
    }
}

impl Command for Connect {
    fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    fn len(&self) -> usize {
        self.addr.len()
    }
}

impl From<Connect> for (Address,) {
    fn from(conn: Connect) -> Self {
        (conn.addr,)
    }
}
