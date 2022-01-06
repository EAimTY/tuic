use super::protocol as socks5_protocol;

impl From<socks5_protocol::Address> for tuic_protocol::Address {
    fn from(addr: socks5_protocol::Address) -> Self {
        match addr {
            socks5_protocol::Address::SocketAddress(addr) => Self::SocketAddress(addr),
            socks5_protocol::Address::UriAuthorityAddress(authority, port) => {
                Self::UriAuthorityAddress(authority, port)
            }
        }
    }
}

impl From<socks5_protocol::Command> for tuic_protocol::Command {
    fn from(cmd: socks5_protocol::Command) -> Self {
        match cmd {
            socks5_protocol::Command::Connect => Self::Connect,
        }
    }
}

impl From<tuic_protocol::Reply> for socks5_protocol::Reply {
    fn from(err: tuic_protocol::Reply) -> Self {
        match err {
            tuic_protocol::Reply::Succeeded => Self::Succeeded,
            tuic_protocol::Reply::GeneralFailure => Self::GeneralFailure,
            tuic_protocol::Reply::ConnectionNotAllowed => Self::ConnectionNotAllowed,
            tuic_protocol::Reply::NetworkUnreachable => Self::NetworkUnreachable,
            tuic_protocol::Reply::HostUnreachable => Self::HostUnreachable,
            tuic_protocol::Reply::ConnectionRefused => Self::ConnectionRefused,
            tuic_protocol::Reply::TtlExpired => Self::TtlExpired,
            tuic_protocol::Reply::CommandNotSupported => Self::CommandNotSupported,
            tuic_protocol::Reply::AddressTypeNotSupported => Self::AddressTypeNotSupported,
            tuic_protocol::Reply::AuthenticationFailed => Self::GeneralFailure,
            tuic_protocol::Reply::Other(code) => Self::Other(code),
        }
    }
}

impl From<tuic_protocol::Error> for socks5_protocol::Error {
    fn from(err: tuic_protocol::Error) -> Self {
        match err {
            tuic_protocol::Error::Io(err) => Self::Io(err),
            tuic_protocol::Error::UnsupportedVersion(version) => Self::UnsupportedVersion(version),
            tuic_protocol::Error::UnsupportedCommand(cmd) => Self::UnsupportedCommand(cmd),
            tuic_protocol::Error::AddressTypeNotSupported(addr_type) => {
                Self::AddressTypeNotSupported(addr_type)
            }
            tuic_protocol::Error::AddressDomainInvalidEncoding => {
                Self::AddressDomainInvalidEncoding
            }
            tuic_protocol::Error::Reply(reply) => Self::Reply(reply.into()),
        }
    }
}
