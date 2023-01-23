// +-+
// | |
// +-+
// | |
// +-+
#[derive(Clone, Debug)]
pub struct Heartbeat;

impl Heartbeat {
    const CMD_TYPE: u8 = 0x04;

    pub fn new() -> Self {
        Self
    }

    pub const fn cmd_type() -> u8 {
        Self::CMD_TYPE
    }

    pub fn len(&self) -> usize {
        0
    }
}
