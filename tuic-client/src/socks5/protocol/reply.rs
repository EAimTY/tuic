use std::fmt;

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
    const REPLY_SUCCEEDED: u8 = 0x00;
    const REPLY_GENERAL_FAILURE: u8 = 0x01;
    const REPLY_CONNECTION_NOT_ALLOWED: u8 = 0x02;
    const REPLY_NETWORK_UNREACHABLE: u8 = 0x03;
    const REPLY_HOST_UNREACHABLE: u8 = 0x04;
    const REPLY_CONNECTION_REFUSED: u8 = 0x05;
    const REPLY_TTL_EXPIRED: u8 = 0x06;
    const REPLY_COMMAND_NOT_SUPPORTED: u8 = 0x07;
    const REPLY_ADDRESS_TYPE_NOT_SUPPORTED: u8 = 0x08;
    
    #[inline]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Succeeded => Self::REPLY_SUCCEEDED,
            Self::GeneralFailure => Self::REPLY_GENERAL_FAILURE,
            Self::ConnectionNotAllowed => Self::REPLY_CONNECTION_NOT_ALLOWED,
            Self::NetworkUnreachable => Self::REPLY_NETWORK_UNREACHABLE,
            Self::HostUnreachable => Self::REPLY_HOST_UNREACHABLE,
            Self::ConnectionRefused => Self::REPLY_CONNECTION_REFUSED,
            Self::TtlExpired => Self::REPLY_TTL_EXPIRED,
            Self::CommandNotSupported => Self::REPLY_COMMAND_NOT_SUPPORTED,
            Self::AddressTypeNotSupported => Self::REPLY_ADDRESS_TYPE_NOT_SUPPORTED,
            Self::Other(c) => c,
        }
    }

    #[inline]
    pub fn from_u8(code: u8) -> Self {
        match code {
            Self::REPLY_SUCCEEDED => Self::Succeeded,
            Self::REPLY_GENERAL_FAILURE => Self::GeneralFailure,
            Self::REPLY_CONNECTION_NOT_ALLOWED => Self::ConnectionNotAllowed,
            Self::REPLY_NETWORK_UNREACHABLE => Self::NetworkUnreachable,
            Self::REPLY_HOST_UNREACHABLE => Self::HostUnreachable,
            Self::REPLY_CONNECTION_REFUSED => Self::ConnectionRefused,
            Self::REPLY_TTL_EXPIRED => Self::TtlExpired,
            Self::REPLY_COMMAND_NOT_SUPPORTED => Self::CommandNotSupported,
            Self::REPLY_ADDRESS_TYPE_NOT_SUPPORTED => Self::AddressTypeNotSupported,
            _ => Self::Other(code),
        }
    }
}

impl fmt::Display for Reply {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Succeeded => write!(f, "Succeeded"),
            Self::AddressTypeNotSupported => write!(f, "Address type not supported"),
            Self::CommandNotSupported => write!(f, "Command not supported"),
            Self::ConnectionNotAllowed => write!(f, "Connection not allowed"),
            Self::ConnectionRefused => write!(f, "Connection refused"),
            Self::GeneralFailure => write!(f, "General failure"),
            Self::HostUnreachable => write!(f, "Host unreachable"),
            Self::NetworkUnreachable => write!(f, "Network unreachable"),
            Self::TtlExpired => write!(f, "TTL expired"),
            Self::Other(code) => write!(f, "Other reply ({})", code),
        }
    }
}
