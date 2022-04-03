use super::protocol::Address;
use crate::relay::Address as RelayAddress;

impl From<RelayAddress> for Address {
    fn from(address: RelayAddress) -> Self {
        match address {
            RelayAddress::DomainAddress(hostname, port) => Self::HostnameAddress(hostname, port),
            RelayAddress::SocketAddress(socket_addr) => Self::SocketAddress(socket_addr),
        }
    }
}

impl From<Address> for RelayAddress {
    fn from(address: Address) -> Self {
        match address {
            Address::HostnameAddress(hostname, port) => Self::DomainAddress(hostname, port),
            Address::SocketAddress(socket_addr) => Self::SocketAddress(socket_addr),
        }
    }
}
