//! The custom protocol for TUIC

mod address;
mod command;
mod error;
mod reply;
mod request;
mod response;

pub const TUIC_PROTOCOL_VERSION: u8 = 0x01;

pub use crate::{
    address::Address, command::Command, error::Error, reply::Reply, request::Request,
    response::Response,
};
