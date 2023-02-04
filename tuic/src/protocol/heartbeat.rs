// +-+
// | |
// +-+
// | |
// +-+
#[derive(Clone, Debug)]
pub struct Heartbeat;

impl Heartbeat {
    const TYPE_CODE: u8 = 0x04;

    pub const fn new() -> Self {
        Self
    }

    pub const fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        0
    }
}

impl From<Heartbeat> for () {
    fn from(_: Heartbeat) -> Self {}
}
