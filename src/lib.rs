pub mod protocol;

#[cfg(any(feature = "server", feature = "client"))]
mod common;

#[cfg(feature = "server")]
mod server;

#[cfg(feature = "client")]
pub mod client;

#[cfg(any(feature = "server", feature = "client"))]
pub use crate::common::{Packet, PacketBufferError, UdpRelayMode};

#[cfg(feature = "client")]
pub use crate::client::{Client, ClientConfig};
