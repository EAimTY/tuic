use super::protocol::{Address, Error, Reply};
use crate::relay::{AddressRelay, RelayError};

impl From<Address> for AddressRelay {
    fn from(addr: Address) -> Self {
        match addr {
            Address::SocketAddress(addr) => Self::SocketAddress(addr),
            Address::HostnameAddress(authority, port) => Self::HostnameAddress(authority, port),
        }
    }
}

impl From<RelayError> for Error {
    fn from(err: RelayError) -> Self {
        match err {
            RelayError::Connection => Self::Reply(Reply::NetworkUnreachable),
            _ => Self::Reply(Reply::GeneralFailure),
        }
    }
}
