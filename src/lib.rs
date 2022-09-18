pub mod protocol;

#[cfg(any(feature = "server", feature = "client"))]
mod common;

#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "client")]
pub mod client;

#[cfg(any(feature = "server", feature = "client"))]
pub use crate::common::{
    packet::{state as packet_state, Packet, PacketBufferHandle},
    stream::{BiStream, RecvStream, SendStream},
    CongestionControl, UdpRelayMode,
};

#[cfg(feature = "client")]
pub use crate::client::{Client, ClientConfig, ClientError};

#[cfg(feature = "server")]
pub use crate::server::{Server, ServerConfig, ServerError};
