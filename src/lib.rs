pub mod protocol;

#[cfg(any(feature = "server", feature = "client"))]
mod common;

#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "client")]
pub mod client;

#[cfg(any(feature = "server", feature = "client"))]
pub use crate::common::{
    stream::{BiStream, RecvStream, SendStream},
    util::PacketBufferGcHandle,
    CongestionControl, UdpRelayMode,
};

#[cfg(feature = "client")]
pub use crate::client::{Client, ClientConfig, ClientError};

#[cfg(feature = "server")]
pub use crate::server::{Server, ServerConfig, ServerError};
