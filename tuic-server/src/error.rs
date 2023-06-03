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
    #[error("connection timed out")]
    TimedOut,
    #[error("connection locally closed")]
    LocallyClosed,
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("duplicated authentication")]
    DuplicatedAuth,
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
    pub fn is_trivial(&self) -> bool {
        matches!(self, Self::TimedOut | Self::LocallyClosed)
    }
}

impl From<ConnectionError> for Error {
    fn from(err: ConnectionError) -> Self {
        match err {
            ConnectionError::TimedOut => Self::TimedOut,
            ConnectionError::LocallyClosed => Self::LocallyClosed,
            _ => Self::Io(IoError::from(err)),
        }
    }
}
