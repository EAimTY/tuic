use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    net::SocketAddr,
};

/// Address
///
/// ```plain
/// +------+----------+
/// | TYPE |   ADDR   |
/// +------+----------+
/// |  1   | Variable |
/// +------+----------+
/// ```
///
/// The address type can be one of the following:
///
/// 0xff: None
/// 0x00: Fully-qualified domain name (the first byte indicates the length of the domain name)
/// 0x01: IPv4 address
/// 0x02: IPv6 address
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

    pub fn type_code(&self) -> u8 {
        match self {
            Self::None => Self::TYPE_CODE_NONE,
            Self::DomainAddress(_, _) => Self::TYPE_CODE_DOMAIN,
            Self::SocketAddress(addr) => match addr {
                SocketAddr::V4(_) => Self::TYPE_CODE_IPV4,
                SocketAddr::V6(_) => Self::TYPE_CODE_IPV6,
            },
        }
    }

    pub fn len(&self) -> usize {
        1 + match self {
            Address::None => 0,
            Address::DomainAddress(addr, _) => 1 + addr.len() + 2,
            Address::SocketAddress(addr) => match addr {
                SocketAddr::V4(_) => 1 * 4 + 2,
                SocketAddr::V6(_) => 2 * 8 + 2,
            },
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_domain(&self) -> bool {
        matches!(self, Self::DomainAddress(_, _))
    }

    pub fn is_ipv4(&self) -> bool {
        matches!(self, Self::SocketAddress(SocketAddr::V4(_)))
    }

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
