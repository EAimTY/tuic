use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    mem,
    net::SocketAddr,
};

mod authenticate;
mod connect;
mod dissociate;
mod heartbeat;
mod packet;

pub use self::{
    authenticate::Authenticate, connect::Connect, dissociate::Dissociate, heartbeat::Heartbeat,
    packet::Packet,
};

/// The TUIC protocol version
pub const VERSION: u8 = 0x05;

/// The command header for negotiating tasks
/// ```plain
/// +-----+------+----------+
/// | VER | TYPE |   OPT    |
/// +-----+------+----------+
/// |  1  |  1   | Variable |
/// +-----+------+----------+
/// ```
///
/// where:
///
/// - `VER` - the TUIC protocol version
/// - `TYPE` - command type
/// - `OPT` - command type specific data
///
/// ## Command Types
///
/// There are five types of command:
///
/// - `0x00` - `Authenticate` - for authenticating the multiplexed stream
/// - `0x01` - `Connect` - for establishing a TCP relay
/// - `0x02` - `Packet` - for relaying (fragmented part of) a UDP packet
/// - `0x03` - `Dissociate` - for terminating a UDP relaying session
/// - `0x04` - `Heartbeat` - for keeping the QUIC connection alive
///
/// Command `Connect` and `Packet` carry payload (stream / packet fragment)
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Header {
    Authenticate(Authenticate),
    Connect(Connect),
    Packet(Packet),
    Dissociate(Dissociate),
    Heartbeat(Heartbeat),
}

impl Header {
    pub const TYPE_CODE_AUTHENTICATE: u8 = Authenticate::type_code();
    pub const TYPE_CODE_CONNECT: u8 = Connect::type_code();
    pub const TYPE_CODE_PACKET: u8 = Packet::type_code();
    pub const TYPE_CODE_DISSOCIATE: u8 = Dissociate::type_code();
    pub const TYPE_CODE_HEARTBEAT: u8 = Heartbeat::type_code();

    /// Returns the command type code
    pub const fn type_code(&self) -> u8 {
        match self {
            Self::Authenticate(_) => Authenticate::type_code(),
            Self::Connect(_) => Connect::type_code(),
            Self::Packet(_) => Packet::type_code(),
            Self::Dissociate(_) => Dissociate::type_code(),
            Self::Heartbeat(_) => Heartbeat::type_code(),
        }
    }

    /// Returns the serialized length of the command
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        2 + match self {
            Self::Authenticate(auth) => auth.len(),
            Self::Connect(conn) => conn.len(),
            Self::Packet(packet) => packet.len(),
            Self::Dissociate(dissociate) => dissociate.len(),
            Self::Heartbeat(heartbeat) => heartbeat.len(),
        }
    }
}

/// Variable-length field that encodes the network address
///
/// ```plain
/// +------+----------+----------+
/// | TYPE |   ADDR   |   PORT   |
/// +------+----------+----------+
/// |  1   | Variable |    2     |
/// +------+----------+----------+
/// ```
///
/// where:
///
/// - `TYPE` - the address type
/// - `ADDR` - the address
/// - `PORT` - the port
///
/// The address type can be one of the following:
///
/// - `0xff`: None
/// - `0x00`: Fully-qualified domain name (the first byte indicates the length of the domain name)
/// - `0x01`: IPv4 address
/// - `0x02`: IPv6 address
///
/// Address type `None` is used in `Packet` commands that is not the first fragment of a UDP packet.
///
/// The port number is encoded in 2 bytes after the Domain name / IP address.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Address {
    None,
    DomainAddress(String, u16),
    SocketAddress(SocketAddr),
}

impl Address {
    pub const TYPE_CODE_NONE: u8 = 0xff;
    pub const TYPE_CODE_DOMAIN: u8 = 0x00;
    pub const TYPE_CODE_IPV4: u8 = 0x01;
    pub const TYPE_CODE_IPV6: u8 = 0x02;

    /// Returns the address type code
    pub const fn type_code(&self) -> u8 {
        match self {
            Self::None => Self::TYPE_CODE_NONE,
            Self::DomainAddress(_, _) => Self::TYPE_CODE_DOMAIN,
            Self::SocketAddress(addr) => match addr {
                SocketAddr::V4(_) => Self::TYPE_CODE_IPV4,
                SocketAddr::V6(_) => Self::TYPE_CODE_IPV6,
            },
        }
    }

    /// Returns the serialized length of the address
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        1 + match self {
            Address::None => 0,
            Address::DomainAddress(addr, _) => 1 + addr.len() + 2,
            Address::SocketAddress(SocketAddr::V4(_)) => 4 + 2,
            Address::SocketAddress(SocketAddr::V6(_)) => 16 + 2,
        }
    }

    /// Takes the address out, leaving a `None` in its place
    pub fn take(&mut self) -> Self {
        mem::take(self)
    }

    /// Returns `true` if the address is `None`
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if the address is a fully-qualified domain name
    pub fn is_domain(&self) -> bool {
        matches!(self, Self::DomainAddress(_, _))
    }

    /// Returns `true` if the address is an IPv4 address
    pub fn is_ipv4(&self) -> bool {
        matches!(self, Self::SocketAddress(SocketAddr::V4(_)))
    }

    /// Returns `true` if the address is an IPv6 address
    pub fn is_ipv6(&self) -> bool {
        matches!(self, Self::SocketAddress(SocketAddr::V6(_)))
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::None => write!(f, "none"),
            Self::DomainAddress(addr, port) => write!(f, "{addr}:{port}"),
            Self::SocketAddress(addr) => write!(f, "{addr}"),
        }
    }
}

impl Default for Address {
    fn default() -> Self {
        Self::None
    }
}
