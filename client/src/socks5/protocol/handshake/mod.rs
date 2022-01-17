use super::{Error, SOCKS5_VERSION};

mod password_request;
mod password_response;
mod request;
mod response;

pub use self::{
    password_request::HandshakePasswordRequest, password_response::HandshakePasswordResponse,
    request::HandshakeRequest, response::HandshakeResponse,
};

pub const SOCKS5_SUBNEGOTIATION_VERSION: u8 = 0x01;

#[derive(Clone)]
pub enum Socks5AuthMethod {
    NONE,
    // GSSAPI,
    PASSWORD { username: String, password: String },
    UNACCEPTABLE,
}
impl Socks5AuthMethod {
    pub fn as_u8(&self) -> u8 {
        match self {
            Socks5AuthMethod::NONE => 0x00,
            // Socks5AuthMethod::GSSAPI => 0x01,
            Socks5AuthMethod::PASSWORD { .. } => 0x02,
            Socks5AuthMethod::UNACCEPTABLE => 0xff,
        }
    }
}
// TODO: implement GSSAPI

pub enum Socks5PasswordAuthStatus {
    SUCCESS,
    FAILED,
}

impl Socks5PasswordAuthStatus {
    pub fn as_u8(&self) -> u8 {
        match self {
            Socks5PasswordAuthStatus::SUCCESS => 0x00,
            Socks5PasswordAuthStatus::FAILED => 0xff,
        }
    }
}
