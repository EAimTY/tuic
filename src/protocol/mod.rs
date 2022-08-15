//! The TUIC protocol

#[cfg(feature = "protocol_marshaling")]
mod marshaling;

use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    net::SocketAddr,
};
use thiserror::Error;

pub const TUIC_PROTOCOL_VERSION: u8 = 0x05;

#[cfg(feature = "protocol_marshaling")]
pub use self::marshaling::MarshalingError;

/// Command
///
/// ```plain
/// +-----+------+----------+
/// | VER | TYPE |   OPT    |
/// +-----+------+----------+
/// |  1  |  1   | Variable |
/// +-----+------+----------+
/// ```
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Command {
    // +-----+
    // | REP |
    // +-----+
    // |  1  |
    // +-----+
    Response(bool),

    // +-----+
    // | TKN |
    // +-----+
    // | 32  |
    // +-----+
    Authenticate([u8; 32]),

    // +----------+
    // |   ADDR   |
    // +----------+
    // | Variable |
    // +----------+
    Connect {
        addr: Address,
    },

    // +----------+--------+------------+---------+-----+----------+
    // | ASSOC_ID | PKT_ID | FRAG_TOTAL | FRAG_ID | LEN |   ADDR   |
    // +----------+--------+------------+---------+-----+----------+
    // |    4     |   2    |     1      |    1    |  2  | Variable |
    // +----------+--------+------------+---------+-----+----------+
    Packet {
        assoc_id: u32,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        len: u16,
        addr: Option<Address>,
    },

    // +----------+
    // | ASSOC_ID |
    // +----------+
    // |    4     |
    // +----------+
    Dissociate {
        assoc_id: u32,
    },

    // +-+
    // | |
    // +-+
    // | |
    // +-+
    Heartbeat,
}

impl Command {
    pub const TYPE_RESPONSE: u8 = 0xff;
    pub const TYPE_AUTHENTICATE: u8 = 0x00;
    pub const TYPE_CONNECT: u8 = 0x01;
    pub const TYPE_PACKET: u8 = 0x02;
    pub const TYPE_DISSOCIATE: u8 = 0x03;
    pub const TYPE_HEARTBEAT: u8 = 0x04;

    pub const RESPONSE_SUCCEEDED: u8 = 0x00;
    pub const RESPONSE_FAILED: u8 = 0xff;

    pub const fn as_type_code(&self) -> u8 {
        match self {
            Command::Response(_) => Self::TYPE_RESPONSE,
            Command::Authenticate(_) => Self::TYPE_AUTHENTICATE,
            Command::Connect { .. } => Self::TYPE_CONNECT,
            Command::Packet { .. } => Self::TYPE_PACKET,
            Command::Dissociate { .. } => Self::TYPE_DISSOCIATE,
            Command::Heartbeat => Self::TYPE_HEARTBEAT,
        }
    }

    pub fn serialized_len(&self) -> usize {
        2 + match self {
            Self::Response(_) => 1,
            Self::Authenticate { .. } => 32,
            Self::Connect { addr } => addr.serialized_len(),
            Self::Packet { addr, .. } => 10 + addr.as_ref().map_or(0, |addr| addr.serialized_len()),
            Self::Dissociate { .. } => 4,
            Self::Heartbeat => 0,
        }
    }
}

/// Address
///
/// ```plain
/// +------+----------+----------+
/// | TYPE |   ADDR   |   PORT   |
/// +------+----------+----------+
/// |  1   | Variable |    2     |
/// +------+----------+----------+
/// ```
///
/// The address type can be one of the following:
/// 0x00: fully-qualified domain name (the first byte indicates the length of the domain name)
/// 0x01: IPv4 address
/// 0x02: IPv6 address
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Address {
    DomainAddress(String, u16),
    SocketAddress(SocketAddr),
}

impl Address {
    pub const TYPE_DOMAIN: u8 = 0x00;
    pub const TYPE_IPV4: u8 = 0x01;
    pub const TYPE_IPV6: u8 = 0x02;

    pub fn serialized_len(&self) -> usize {
        1 + match self {
            Address::DomainAddress(addr, _) => 1 + addr.len() + 2,
            Address::SocketAddress(addr) => match addr {
                SocketAddr::V4(_) => 6,
                SocketAddr::V6(_) => 18,
            },
        }
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::DomainAddress(addr, port) => write!(f, "{addr}:{port}"),
            Self::SocketAddress(addr) => write!(f, "{addr}"),
        }
    }
}

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("unsupported TUIC version: {0:#x}")]
    UnsupportedVersion(u8),
    #[error("invalid command: {0:#x}")]
    InvalidCommand(u8),
    #[error("invalid response: {0:#x}")]
    InvalidResponse(u8),
    #[error("invalid address type: {0:#x}")]
    InvalidAddressType(u8),
}
