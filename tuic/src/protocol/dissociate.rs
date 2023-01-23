use super::Command;

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
    pub const TYPE_CODE: u8 = 0x03;

    pub fn new(assoc_id: u16) -> Self {
        Self { assoc_id }
    }
}

impl Command for Dissociate {
    fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    fn len(&self) -> usize {
        2
    }
}
