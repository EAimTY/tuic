use std::fmt;

pub const SOCKS5_REPLY_SUCCEEDED: u8 = 0x00;
pub const SOCKS5_REPLY_GENERAL_FAILURE: u8 = 0x01;
pub const SOCKS5_REPLY_CONNECTION_NOT_ALLOWED: u8 = 0x02;
pub const SOCKS5_REPLY_NETWORK_UNREACHABLE: u8 = 0x03;
pub const SOCKS5_REPLY_HOST_UNREACHABLE: u8 = 0x04;
pub const SOCKS5_REPLY_CONNECTION_REFUSED: u8 = 0x05;
pub const SOCKS5_REPLY_TTL_EXPIRED: u8 = 0x06;
pub const SOCKS5_REPLY_COMMAND_NOT_SUPPORTED: u8 = 0x07;
pub const SOCKS5_REPLY_ADDRESS_TYPE_NOT_SUPPORTED: u8 = 0x08;

#[derive(Clone, Copy, Debug)]
pub enum Reply {
    Succeeded,
    GeneralFailure,
    ConnectionNotAllowed,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TtlExpired,
    CommandNotSupported,
    AddressTypeNotSupported,
    Other(u8),
}

impl Reply {
    #[inline]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Succeeded => SOCKS5_REPLY_SUCCEEDED,
            Self::GeneralFailure => SOCKS5_REPLY_GENERAL_FAILURE,
            Self::ConnectionNotAllowed => SOCKS5_REPLY_CONNECTION_NOT_ALLOWED,
            Self::NetworkUnreachable => SOCKS5_REPLY_NETWORK_UNREACHABLE,
            Self::HostUnreachable => SOCKS5_REPLY_HOST_UNREACHABLE,
            Self::ConnectionRefused => SOCKS5_REPLY_CONNECTION_REFUSED,
            Self::TtlExpired => SOCKS5_REPLY_TTL_EXPIRED,
            Self::CommandNotSupported => SOCKS5_REPLY_COMMAND_NOT_SUPPORTED,
            Self::AddressTypeNotSupported => SOCKS5_REPLY_ADDRESS_TYPE_NOT_SUPPORTED,
            Self::Other(c) => c,
        }
    }

    #[inline]
    pub fn from_u8(code: u8) -> Self {
        match code {
            SOCKS5_REPLY_SUCCEEDED => Self::Succeeded,
            SOCKS5_REPLY_GENERAL_FAILURE => Self::GeneralFailure,
            SOCKS5_REPLY_CONNECTION_NOT_ALLOWED => Self::ConnectionNotAllowed,
            SOCKS5_REPLY_NETWORK_UNREACHABLE => Self::NetworkUnreachable,
            SOCKS5_REPLY_HOST_UNREACHABLE => Self::HostUnreachable,
            SOCKS5_REPLY_CONNECTION_REFUSED => Self::ConnectionRefused,
            SOCKS5_REPLY_TTL_EXPIRED => Self::TtlExpired,
            SOCKS5_REPLY_COMMAND_NOT_SUPPORTED => Self::CommandNotSupported,
            SOCKS5_REPLY_ADDRESS_TYPE_NOT_SUPPORTED => Self::AddressTypeNotSupported,
            _ => Self::Other(code),
        }
    }
}

impl fmt::Display for Reply {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Succeeded => write!(f, "Succeeded"),
            Self::AddressTypeNotSupported => write!(f, "Address type not supported"),
            Self::CommandNotSupported => write!(f, "Command not supported"),
            Self::ConnectionNotAllowed => write!(f, "Connection not allowed"),
            Self::ConnectionRefused => write!(f, "Connection refused"),
            Self::GeneralFailure => write!(f, "General failure"),
            Self::HostUnreachable => write!(f, "Host unreachable"),
            Self::NetworkUnreachable => write!(f, "Network unreachable"),
            Self::Other(u) => write!(f, "Other reply ({})", u),
            Self::TtlExpired => write!(f, "TTL expired"),
        }
    }
}
