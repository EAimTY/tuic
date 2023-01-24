use super::{Address, Command};

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
    pub(super) const TYPE_CODE: u8 = 0x01;

    pub const fn new(addr: Address) -> Self {
        Self { addr }
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
