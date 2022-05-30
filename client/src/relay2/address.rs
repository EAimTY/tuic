use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    net::SocketAddr,
};
use tuic_protocol::Address as TuicAddress;

pub enum Address {
    DomainAddress(String, u16),
    SocketAddress(SocketAddr),
}

impl From<TuicAddress> for Address {
    fn from(address: TuicAddress) -> Self {
        match address {
            TuicAddress::DomainAddress(hostname, port) => Self::DomainAddress(hostname, port),
            TuicAddress::SocketAddress(socket_addr) => Self::SocketAddress(socket_addr),
        }
    }
}

impl From<Address> for TuicAddress {
    fn from(address: Address) -> Self {
        match address {
            Address::DomainAddress(hostname, port) => Self::DomainAddress(hostname, port),
            Address::SocketAddress(socket_addr) => Self::SocketAddress(socket_addr),
        }
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Address::DomainAddress(hostname, port) => write!(f, "{hostname}:{port}"),
            Address::SocketAddress(socket_addr) => write!(f, "{socket_addr}"),
        }
    }
}
