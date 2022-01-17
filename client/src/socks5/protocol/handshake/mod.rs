use super::{Error, SOCKS5_VERSION};

pub mod password;
mod request;
mod response;

pub use self::{request::HandshakeRequest, response::HandshakeResponse};

pub enum Authentication {
    None,
    // GSSAPI,
    Password {
        username: Vec<u8>,
        password: Vec<u8>,
    },
    Unacceptable,
}

impl Authentication {
    const AUTH_METHOD_NONE: u8 = 0x00;
    // const AUTH_METHOD_GSSAPI: u8 = 0x01;
    const AUTH_METHOD_PASSWORD: u8 = 0x02;
    const AUTH_METHOD_UNACCEPTABLE: u8 = 0xff;

    #[inline]
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::None => Self::AUTH_METHOD_NONE,
            Self::Password { .. } => Self::AUTH_METHOD_PASSWORD,
            Self::Unacceptable => Self::AUTH_METHOD_UNACCEPTABLE,
        }
    }
}
