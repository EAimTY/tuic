//! The TUIC protocol

mod protocol;

pub use self::protocol::{
    Address, Authenticate, Connect, Dissociate, Header, Heartbeat, Packet, VERSION,
};

#[cfg(feature = "async_marshal")]
mod marshal;

#[cfg(feature = "async_marshal")]
mod unmarshal;

#[cfg(feature = "async_marshal")]
pub use self::unmarshal::UnmarshalError;

#[cfg(feature = "model")]
pub mod model;
