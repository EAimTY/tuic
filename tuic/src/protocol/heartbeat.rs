/// Command `Heartbeat`
/// ```plain
/// +-+
/// | |
/// +-+
/// | |
/// +-+
/// ```
#[derive(Clone, Debug)]
pub struct Heartbeat;

impl Heartbeat {
    const TYPE_CODE: u8 = 0x04;

    /// Creates a new `Heartbeat` command
    pub const fn new() -> Self {
        Self
    }

    /// Returns the command type code
    pub const fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    /// Returns the serialized length of the command
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        0
    }
}

impl From<Heartbeat> for () {
    fn from(_: Heartbeat) -> Self {}
}
