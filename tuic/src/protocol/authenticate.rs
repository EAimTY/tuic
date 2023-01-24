use super::Command;

// +-------+
// | TOKEN |
// +-------+
// |   8   |
// +-------+
#[derive(Clone, Debug)]
pub struct Authenticate {
    pub token: [u8; 8],
}

impl Authenticate {
    pub(super) const TYPE_CODE: u8 = 0x00;

    pub const fn new(token: [u8; 8]) -> Self {
        Self { token }
    }
}

impl Command for Authenticate {
    fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    fn len(&self) -> usize {
        8
    }
}
