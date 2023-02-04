// +-------+
// | TOKEN |
// +-------+
// |  32   |
// +-------+
#[derive(Clone, Debug)]
pub struct Authenticate {
    token: [u8; 32],
}

impl Authenticate {
    const TYPE_CODE: u8 = 0x00;

    pub const fn new(token: [u8; 32]) -> Self {
        Self { token }
    }

    pub fn token(&self) -> [u8; 32] {
        self.token
    }

    pub const fn type_code() -> u8 {
        Self::TYPE_CODE
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        32
    }
}

impl From<Authenticate> for ([u8; 32],) {
    fn from(auth: Authenticate) -> Self {
        (auth.token,)
    }
}
