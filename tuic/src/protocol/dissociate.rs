// +----------+
// | ASSOC_ID |
// +----------+
// |    2     |
// +----------+
#[derive(Clone, Debug)]
pub struct Dissociate {
    pub assoc_id: u16,
}

impl Dissociate {
    const CMD_TYPE: u8 = 0x03;

    pub fn new(assoc_id: u16) -> Self {
        Self { assoc_id }
    }

    pub const fn cmd_type() -> u8 {
        Self::CMD_TYPE
    }

    pub fn len(&self) -> usize {
        2
    }
}
