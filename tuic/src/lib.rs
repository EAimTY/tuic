//! The TUIC protocol

mod protocol;

#[cfg(feature = "marshal")]
mod marshal;

#[cfg(feature = "marshal")]
mod unmarshal;

pub use self::protocol::{
    Address, Authenticate, Command, Connect, Dissociate, Header, Heartbeat, Packet, VERSION,
};

#[cfg(feature = "marshal")]
pub use self::{
    marshal::Marshal,
    unmarshal::{Unmarshal, UnmarshalError},
};

#[cfg(feature = "model")]
pub mod model;
