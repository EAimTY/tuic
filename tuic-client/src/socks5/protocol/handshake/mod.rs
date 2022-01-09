use super::{Error, SOCKS5_VERSION};

mod request;
mod response;

pub use self::{request::HandshakeRequest, response::HandshakeResponse};

pub const SOCKS5_AUTH_METHOD_NONE: u8 = 0x00;
// pub const SOCKS5_AUTH_METHOD_GSSAPI: u8 = 0x01;
// pub const SOCKS5_AUTH_METHOD_PASSWORD: u8 = 0x02;
// pub const SOCKS5_AUTH_METHOD_NOT_ACCEPTABLE: u8 = 0xff;
