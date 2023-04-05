use crate::{Address, Authenticate, Connect, Dissociate, Header, Heartbeat, Packet, VERSION};
use futures_util::{AsyncRead, AsyncReadExt};
use std::{
    io::{Error as IoError, Read},
    net::SocketAddr,
    string::FromUtf8Error,
};
use thiserror::Error;
use uuid::{Error as UuidError, Uuid};

impl Header {
    /// Unmarshals a header from an `AsyncRead` stream
    #[cfg(feature = "async_marshal")]
    pub async fn async_unmarshal(s: &mut (impl AsyncRead + Unpin)) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 1];
        s.read_exact(&mut buf).await?;
        let ver = buf[0];

        if ver != VERSION {
            return Err(UnmarshalError::InvalidVersion(ver));
        }

        let mut buf = [0; 1];
        s.read_exact(&mut buf).await?;
        let cmd = buf[0];

        match cmd {
            Header::TYPE_CODE_AUTHENTICATE => {
                Authenticate::async_read(s).await.map(Self::Authenticate)
            }
            Header::TYPE_CODE_CONNECT => Connect::async_read(s).await.map(Self::Connect),
            Header::TYPE_CODE_PACKET => Packet::async_read(s).await.map(Self::Packet),
            Header::TYPE_CODE_DISSOCIATE => Dissociate::async_read(s).await.map(Self::Dissociate),
            Header::TYPE_CODE_HEARTBEAT => Heartbeat::async_read(s).await.map(Self::Heartbeat),
            _ => Err(UnmarshalError::InvalidCommand(cmd)),
        }
    }

    /// Unmarshals a header from a `Read` stream
    #[cfg(feature = "marshal")]
    pub fn unmarshal(s: &mut impl Read) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 1];
        s.read_exact(&mut buf)?;
        let ver = buf[0];

        if ver != VERSION {
            return Err(UnmarshalError::InvalidVersion(ver));
        }

        let mut buf = [0; 1];
        s.read_exact(&mut buf)?;
        let cmd = buf[0];

        match cmd {
            Header::TYPE_CODE_AUTHENTICATE => Authenticate::read(s).map(Self::Authenticate),
            Header::TYPE_CODE_CONNECT => Connect::read(s).map(Self::Connect),
            Header::TYPE_CODE_PACKET => Packet::read(s).map(Self::Packet),
            Header::TYPE_CODE_DISSOCIATE => Dissociate::read(s).map(Self::Dissociate),
            Header::TYPE_CODE_HEARTBEAT => Heartbeat::read(s).map(Self::Heartbeat),
            _ => Err(UnmarshalError::InvalidCommand(cmd)),
        }
    }
}

impl Address {
    #[cfg(feature = "async_marshal")]
    async fn async_read(s: &mut (impl AsyncRead + Unpin)) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 1];
        s.read_exact(&mut buf).await?;
        let type_code = buf[0];

        match type_code {
            Address::TYPE_CODE_NONE => Ok(Self::None),
            Address::TYPE_CODE_DOMAIN => {
                let mut buf = [0; 1];
                s.read_exact(&mut buf).await?;
                let len = buf[0] as usize;

                let mut buf = vec![0; len + 2];
                s.read_exact(&mut buf).await?;
                let port = u16::from_be_bytes([buf[len], buf[len + 1]]);
                buf.truncate(len);
                let domain = String::from_utf8(buf)?;

                Ok(Self::DomainAddress(domain, port))
            }
            Address::TYPE_CODE_IPV4 => {
                let mut buf = [0; 6];
                s.read_exact(&mut buf).await?;
                let ip = [buf[0], buf[1], buf[2], buf[3]];
                let port = u16::from_be_bytes([buf[4], buf[5]]);
                Ok(Self::SocketAddress(SocketAddr::from((ip, port))))
            }
            Address::TYPE_CODE_IPV6 => {
                let mut buf = [0; 18];
                s.read_exact(&mut buf).await?;
                let ip = [
                    u16::from_be_bytes([buf[0], buf[1]]),
                    u16::from_be_bytes([buf[2], buf[3]]),
                    u16::from_be_bytes([buf[4], buf[5]]),
                    u16::from_be_bytes([buf[6], buf[7]]),
                    u16::from_be_bytes([buf[8], buf[9]]),
                    u16::from_be_bytes([buf[10], buf[11]]),
                    u16::from_be_bytes([buf[12], buf[13]]),
                    u16::from_be_bytes([buf[14], buf[15]]),
                ];
                let port = u16::from_be_bytes([buf[16], buf[17]]);

                Ok(Self::SocketAddress(SocketAddr::from((ip, port))))
            }
            _ => Err(UnmarshalError::InvalidAddressType(type_code)),
        }
    }

    #[cfg(feature = "marshal")]
    fn read(s: &mut impl Read) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 1];
        s.read_exact(&mut buf)?;
        let type_code = buf[0];

        match type_code {
            Address::TYPE_CODE_NONE => Ok(Self::None),
            Address::TYPE_CODE_DOMAIN => {
                let mut buf = [0; 1];
                s.read_exact(&mut buf)?;
                let len = buf[0] as usize;

                let mut buf = vec![0; len + 2];
                s.read_exact(&mut buf)?;
                let port = u16::from_be_bytes([buf[len], buf[len + 1]]);
                buf.truncate(len);
                let domain = String::from_utf8(buf)?;

                Ok(Self::DomainAddress(domain, port))
            }
            Address::TYPE_CODE_IPV4 => {
                let mut buf = [0; 6];
                s.read_exact(&mut buf)?;
                let ip = [buf[0], buf[1], buf[2], buf[3]];
                let port = u16::from_be_bytes([buf[4], buf[5]]);
                Ok(Self::SocketAddress(SocketAddr::from((ip, port))))
            }
            Address::TYPE_CODE_IPV6 => {
                let mut buf = [0; 18];
                s.read_exact(&mut buf)?;
                let ip = [
                    u16::from_be_bytes([buf[0], buf[1]]),
                    u16::from_be_bytes([buf[2], buf[3]]),
                    u16::from_be_bytes([buf[4], buf[5]]),
                    u16::from_be_bytes([buf[6], buf[7]]),
                    u16::from_be_bytes([buf[8], buf[9]]),
                    u16::from_be_bytes([buf[10], buf[11]]),
                    u16::from_be_bytes([buf[12], buf[13]]),
                    u16::from_be_bytes([buf[14], buf[15]]),
                ];
                let port = u16::from_be_bytes([buf[16], buf[17]]);

                Ok(Self::SocketAddress(SocketAddr::from((ip, port))))
            }
            _ => Err(UnmarshalError::InvalidAddressType(type_code)),
        }
    }
}

impl Authenticate {
    #[cfg(feature = "async_marshal")]
    async fn async_read(s: &mut (impl AsyncRead + Unpin)) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 48];
        s.read_exact(&mut buf).await?;
        let uuid = Uuid::from_slice(&buf[..16])?;
        let token = TryFrom::try_from(&buf[16..]).unwrap();
        Ok(Self::new(uuid, token))
    }

    #[cfg(feature = "marshal")]
    fn read(s: &mut impl Read) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 48];
        s.read_exact(&mut buf)?;
        let uuid = Uuid::from_slice(&buf[..16])?;
        let token = TryFrom::try_from(&buf[16..]).unwrap();
        Ok(Self::new(uuid, token))
    }
}

impl Connect {
    #[cfg(feature = "async_marshal")]
    async fn async_read(s: &mut (impl AsyncRead + Unpin)) -> Result<Self, UnmarshalError> {
        Ok(Self::new(Address::async_read(s).await?))
    }

    #[cfg(feature = "marshal")]
    fn read(s: &mut impl Read) -> Result<Self, UnmarshalError> {
        Ok(Self::new(Address::read(s)?))
    }
}

impl Packet {
    #[cfg(feature = "async_marshal")]
    async fn async_read(s: &mut (impl AsyncRead + Unpin)) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 8];
        s.read_exact(&mut buf).await?;

        let assoc_id = u16::from_be_bytes([buf[0], buf[1]]);
        let pkt_id = u16::from_be_bytes([buf[2], buf[3]]);
        let frag_total = buf[4];
        let frag_id = buf[5];
        let size = u16::from_be_bytes([buf[6], buf[7]]);
        let addr = Address::async_read(s).await?;

        Ok(Self::new(assoc_id, pkt_id, frag_total, frag_id, size, addr))
    }

    #[cfg(feature = "marshal")]
    fn read(s: &mut impl Read) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 8];
        s.read_exact(&mut buf)?;

        let assoc_id = u16::from_be_bytes([buf[0], buf[1]]);
        let pkt_id = u16::from_be_bytes([buf[2], buf[3]]);
        let frag_total = buf[4];
        let frag_id = buf[5];
        let size = u16::from_be_bytes([buf[6], buf[7]]);
        let addr = Address::read(s)?;

        Ok(Self::new(assoc_id, pkt_id, frag_total, frag_id, size, addr))
    }
}

impl Dissociate {
    #[cfg(feature = "async_marshal")]
    async fn async_read(s: &mut (impl AsyncRead + Unpin)) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 2];
        s.read_exact(&mut buf).await?;
        let assoc_id = u16::from_be_bytes(buf);
        Ok(Self::new(assoc_id))
    }

    #[cfg(feature = "marshal")]
    fn read(s: &mut impl Read) -> Result<Self, UnmarshalError> {
        let mut buf = [0; 2];
        s.read_exact(&mut buf)?;
        let assoc_id = u16::from_be_bytes(buf);
        Ok(Self::new(assoc_id))
    }
}

impl Heartbeat {
    #[cfg(feature = "async_marshal")]
    async fn async_read(_s: &mut (impl AsyncRead + Unpin)) -> Result<Self, UnmarshalError> {
        Ok(Self::new())
    }

    #[cfg(feature = "marshal")]
    fn read(_s: &mut impl Read) -> Result<Self, UnmarshalError> {
        Ok(Self::new())
    }
}

/// Errors that can occur when unmarshalling a packet
#[derive(Debug, Error)]
pub enum UnmarshalError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error("invalid version: {0}")]
    InvalidVersion(u8),
    #[error("invalid command: {0}")]
    InvalidCommand(u8),
    #[error("invalid UUID: {0}")]
    InvalidUuid(#[from] UuidError),
    #[error("invalid address type: {0}")]
    InvalidAddressType(u8),
    #[error("address parsing error: {0}")]
    AddressParse(#[from] FromUtf8Error),
}
