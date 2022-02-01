#[derive(Clone, Copy, Debug)]
pub enum HandshakeMethod {
    None,
    Gssapi,
    Password,
    Unacceptable,
    Other(u8),
}

impl HandshakeMethod {
    const METHOD_NONE: u8 = 0x00;
    const METHOD_GSSAPI: u8 = 0x01;
    const METHOD_PASSWORD: u8 = 0x02;
    const METHOD_UNACCEPTABLE: u8 = 0xff;

    #[inline]
    pub fn from_u8(code: u8) -> Self {
        match code {
            Self::METHOD_NONE => Self::None,
            Self::METHOD_GSSAPI => Self::Gssapi,
            Self::METHOD_PASSWORD => Self::Password,
            Self::METHOD_UNACCEPTABLE => Self::Unacceptable,
            code => Self::Other(code),
        }
    }

    #[inline]
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::None => Self::METHOD_NONE,
            Self::Gssapi => Self::METHOD_GSSAPI,
            Self::Password => Self::METHOD_PASSWORD,
            Self::Unacceptable => Self::METHOD_UNACCEPTABLE,
            Self::Other(code) => *code,
        }
    }
}
