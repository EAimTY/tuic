//! The custom protocol for tuic

mod address;
mod command;
mod connect;
mod error;
mod handshake;
mod reply;

pub const TUIC_PROTOCOL_VERSION: u8 = 1;

pub use crate::{
    address::Address,
    command::Command,
    connect::{ConnectRequest, ConnectResponse},
    error::Error,
    handshake::{HandshakeRequest, HandshakeResponse},
    reply::Reply,
};
