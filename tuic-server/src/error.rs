use quinn::ConnectionError;
use rustls::Error as RustlsError;
use std::{io::Error as IoError, net::SocketAddr};
use thiserror::Error;
use tuic_quinn::Error as ModelError;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Rustls(#[from] RustlsError),
    #[error("invalid max idle time")]
    InvalidMaxIdleTime,
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("duplicated authentication")]
    DuplicatedAuth,
    #[error("token length too short")]
    ExportKeyingMaterial,
    #[error("authentication failed: {0}")]
    AuthFailed(Uuid),
    #[error("received packet from unexpected source")]
    UnexpectedPacketSource,
    #[error("{0}: {1}")]
    Socket(&'static str, IoError),
    #[error("task negotiation timed out")]
    TaskNegotiationTimeout,
    #[error("failed sending packet to {0}: relaying IPv6 UDP packet is disabled")]
    UdpRelayIpv6Disabled(SocketAddr),
}

impl Error {
    pub fn is_locally_closed(&self) -> bool {
        matches!(self, Self::Connection(ConnectionError::LocallyClosed))
    }

    pub fn is_timeout_closed(&self) -> bool {
        matches!(self, Self::Connection(ConnectionError::TimedOut))
    }
}
