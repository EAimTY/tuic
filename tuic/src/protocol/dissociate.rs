use super::Command;

// +----------+
// | ASSOC_ID |
// +----------+
// |    2     |
// +----------+
#[derive(Clone, Debug)]
pub struct Dissociate {
    assoc_id: u16,
}

impl Dissociate {
    pub(super) const TYPE_CODE: u8 = 0x03;

    pub const fn new(assoc_id: u16) -> Self {
        Self { assoc_id }
    }

    pub fn assoc_id(&self) -> u16 {
        self.assoc_id
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
