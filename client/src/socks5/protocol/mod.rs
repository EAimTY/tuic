mod address;
mod command;
mod error;
pub mod handshake;
mod reply;
mod request;
mod response;

pub const SOCKS5_VERSION: u8 = 0x05;

pub use self::{
    address::Address,
    command::Command,
    error::Error,
    handshake::{HandshakeRequest, HandshakeResponse},
    reply::Reply,
    request::Request,
    response::Response,
};
