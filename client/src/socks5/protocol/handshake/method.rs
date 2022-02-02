#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct HandshakeMethod(pub u8);

impl HandshakeMethod {
    pub const NONE: Self = Self(0x00);
    pub const GSSAPI: Self = Self(0x01);
    pub const PASSWORD: Self = Self(0x02);
    pub const UNACCEPTABLE: Self = Self(0xff);

    #[inline]
    pub fn as_u8(self) -> u8 {
        self.0
    }
}
