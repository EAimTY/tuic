use super::Address;

/// Command `Connect`
/// ```plain
/// +----------+
/// |   ADDR   |
/// +----------+
/// | Variable |
/// +----------+
/// ```
///
/// where:
///
/// - `ADDR` - target address
#[derive(Clone, Debug)]
pub struct Connect {
    addr: Address,
}

impl Connect {
    const TYPE_CODE: u8 = 0x01;

    /// Creates a new `Connect` command
    pub const fn new(addr: Address) -> Self {
        Self { addr }
    }

    /// Returns the address
    pub fn addr(&self) -> &Address {
        &self.addr
    }

    /// Returns the command type code
    pub const fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    /// Returns the serialized length of the command
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.addr.len()
    }
}

impl From<Connect> for (Address,) {
    fn from(conn: Connect) -> Self {
        (conn.addr,)
    }
}
