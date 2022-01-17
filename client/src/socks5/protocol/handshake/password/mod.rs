use super::Error;

mod request;
mod response;

pub const SUBNEGOTIATION_VERSION: u8 = 0x01;

pub use self::{request::Request, response::Response};
