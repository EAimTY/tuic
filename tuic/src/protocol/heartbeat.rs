use super::Command;

// +-+
// | |
// +-+
// | |
// +-+
#[derive(Clone, Debug)]
pub struct Heartbeat;

impl Heartbeat {
    pub const TYPE_CODE: u8 = 0x04;

    pub fn new() -> Self {
        Self
    }
}

impl Command for Heartbeat {
    fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    fn len(&self) -> usize {
        0
    }
}
