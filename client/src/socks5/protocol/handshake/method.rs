#[derive(Clone, Copy, PartialEq)]
#[repr(transparent)]
pub struct HandshakeMethod(pub u8);

#[allow(non_upper_case_globals)]
impl HandshakeMethod {
    pub const None: Self = Self(0x00);
    pub const Gssapi: Self = Self(0x01);
    pub const Password: Self = Self(0x02);
    pub const Unacceptable: Self = Self(0xff);

    pub fn as_u8(self) -> u8 {
        self.0
    }
}
