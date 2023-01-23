use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    net::SocketAddr,
};

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
    pub const TYPE_CODE_DOMAIN: u8 = 0x00;
    pub const TYPE_CODE_IPV4: u8 = 0x01;
    pub const TYPE_CODE_IPV6: u8 = 0x02;

    pub fn type_code(&self) -> u8 {
        match self {
            Self::DomainAddress(_, _) => Self::TYPE_CODE_DOMAIN,
            Self::SocketAddress(addr) => match addr {
                SocketAddr::V4(_) => Self::TYPE_CODE_IPV4,
                SocketAddr::V6(_) => Self::TYPE_CODE_IPV6,
            },
        }
    }

    pub fn len(&self) -> usize {
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
