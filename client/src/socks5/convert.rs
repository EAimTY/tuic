use super::protocol::{
    Address as Socks5Address, Command as Socks5Command, Error as Socks5Error, Reply as Socks5Reply,
};
use tuic_protocol::{
    Address as TuicAddress, Command as TuicCommand, Error as TuicError, Reply as TuicReply,
};

impl From<Socks5Address> for TuicAddress {
    fn from(addr: Socks5Address) -> Self {
        match addr {
            Socks5Address::SocketAddress(addr) => Self::SocketAddress(addr),
            Socks5Address::HostnameAddress(authority, port) => {
                Self::HostnameAddress(authority, port)
            }
        }
    }
}

impl From<Socks5Command> for TuicCommand {
    fn from(cmd: Socks5Command) -> Self {
        match cmd {
            Socks5Command::Connect => Self::Connect,
        }
    }
}

impl From<TuicReply> for Socks5Reply {
    fn from(err: TuicReply) -> Self {
        match err {
            TuicReply::Succeeded => Self::Succeeded,
            TuicReply::GeneralFailure => Self::GeneralFailure,
            TuicReply::ConnectionNotAllowed => Self::ConnectionNotAllowed,
            TuicReply::NetworkUnreachable => Self::NetworkUnreachable,
            TuicReply::HostUnreachable => Self::HostUnreachable,
            TuicReply::ConnectionRefused => Self::ConnectionRefused,
            TuicReply::TtlExpired => Self::TtlExpired,
            TuicReply::CommandNotSupported => Self::CommandNotSupported,
            TuicReply::AddressTypeNotSupported => Self::AddressTypeNotSupported,
            TuicReply::AuthenticationFailed => Self::GeneralFailure,
            TuicReply::Other(code) => Self::Other(code),
        }
    }
}

impl From<TuicError> for Socks5Error {
    fn from(err: TuicError) -> Self {
        match err {
            TuicError::Io(err) => Self::Io(err),
            TuicError::UnsupportedVersion(_) => Self::Reply(Socks5Reply::GeneralFailure),
            TuicError::UnsupportedCommand(cmd) => Self::UnsupportedCommand(cmd),
            TuicError::AddressTypeNotSupported(addr_type) => {
                Self::AddressTypeNotSupported(addr_type)
            }
            TuicError::AddressDomainInvalidEncoding => Self::AddressDomainInvalidEncoding,
            TuicError::Reply(reply) => Self::Reply(reply.into()),
        }
    }
}
