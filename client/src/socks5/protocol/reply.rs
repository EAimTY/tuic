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
}
