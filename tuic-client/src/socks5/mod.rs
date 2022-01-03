#![allow(unused)]

mod address;
mod command;
mod connect;
mod error;
pub mod handshake;
mod reply;

pub const SOCKS5_VERSION: u8 = 0x05;

pub use self::{
    address::Address,
    command::Command,
    connect::{ConnectRequest, ConnectResponse},
    error::Error,
    handshake::{HandshakeRequest, HandshakeResponse},
    reply::Reply,
};
