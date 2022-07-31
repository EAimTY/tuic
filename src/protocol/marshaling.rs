use super::{Address, Command, TUIC_PROTOCOL_VERSION};
use byteorder::{BigEndian, ReadBytesExt};
use bytes::BufMut;
use std::{
    io::{Cursor, Error as IoError, ErrorKind as IoErrorKind},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    string::FromUtf8Error,
};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

impl Command {
    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let ver = r.read_u8().await?;

        if ver != TUIC_PROTOCOL_VERSION {
            return Err(Error::UnsupportedVersion(ver));
        }

        let cmd = r.read_u8().await?;

        match cmd {
            Self::TYPE_RESPONSE => {
                let resp = r.read_u8().await?;
                match resp {
                    Self::RESPONSE_SUCCEEDED => Ok(Self::Response(true)),
                    Self::RESPONSE_FAILED => Ok(Self::Response(false)),
                    _ => Err(Error::InvalidResponse(resp)),
                }
            }
            Self::TYPE_AUTHENTICATE => {
                let mut digest = [0; 32];
                r.read_exact(&mut digest).await?;
                Ok(Self::Authenticate(digest))
            }
            Self::TYPE_CONNECT => {
                let addr = Address::read_from(r).await?;
                Ok(Self::Connect { addr })
            }
            Self::TYPE_PACKET => {
                let mut buf = [0; 6];
                r.read_exact(&mut buf).await?;
                let mut rdr = Cursor::new(buf);

                let assoc_id = ReadBytesExt::read_u32::<BigEndian>(&mut rdr).unwrap();
                let len = ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap();
                let addr = Address::read_from(r).await?;

                Ok(Self::Packet {
                    assoc_id,
                    len,
                    addr,
                })
            }
            Self::TYPE_DISSOCIATE => {
                let assoc_id = r.read_u32().await?;
                Ok(Self::Dissociate { assoc_id })
            }
            Self::TYPE_HEARTBEAT => Ok(Self::Heartbeat),
            _ => Err(Error::InvalidCommand(cmd)),
        }
    }

    pub async fn write_to<W>(&self, w: &mut W) -> Result<(), Error>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = Vec::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        w.write_all(&buf).await?;
        Ok(())
    }

    pub fn write_to_buf<B: BufMut>(&self, buf: &mut B) {
        buf.put_u8(TUIC_PROTOCOL_VERSION);

        match self {
            Self::Response(is_succeeded) => {
                buf.put_u8(Self::TYPE_RESPONSE);
                if *is_succeeded {
                    buf.put_u8(Self::RESPONSE_SUCCEEDED);
                } else {
                    buf.put_u8(Self::RESPONSE_FAILED);
                }
            }
            Self::Authenticate(digest) => {
                buf.put_u8(Self::TYPE_AUTHENTICATE);
                buf.put_slice(digest);
            }
            Self::Connect { addr } => {
                buf.put_u8(Self::TYPE_CONNECT);
                addr.write_to_buf(buf);
            }
            Self::Packet {
                assoc_id,
                len,
                addr,
            } => {
                buf.put_u8(Self::TYPE_PACKET);
                buf.put_u32(*assoc_id);
                buf.put_u16(*len);
                addr.write_to_buf(buf);
            }
            Self::Dissociate { assoc_id } => {
                buf.put_u8(Self::TYPE_DISSOCIATE);
                buf.put_u32(*assoc_id);
            }
            Self::Heartbeat => {
                buf.put_u8(Self::TYPE_HEARTBEAT);
            }
        }
    }

    pub fn serialized_len(&self) -> usize {
        2 + match self {
            Self::Response(_) => 1,
            Self::Authenticate { .. } => 32,
            Self::Connect { addr } => addr.serialized_len(),
            Self::Packet { addr, .. } => 6 + addr.serialized_len(),
            Self::Dissociate { .. } => 4,
            Self::Heartbeat => 0,
        }
    }
}

impl Address {
    pub async fn read_from<R>(stream: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let addr_type = stream.read_u8().await?;

        match addr_type {
            Self::TYPE_DOMAIN => {
                let len = stream.read_u8().await? as usize;

                let mut buf = vec![0; len + 2];
                stream.read_exact(&mut buf).await?;

                let port = ReadBytesExt::read_u16::<BigEndian>(&mut &buf[len..]).unwrap();
                buf.truncate(len);

                let addr = String::from_utf8(buf)?;

                Ok(Self::DomainAddress(addr, port))
            }
            Self::TYPE_IPV4 => {
                let mut buf = [0; 6];
                stream.read_exact(&mut buf).await?;
                let mut rdr = Cursor::new(buf);

                let addr = Ipv4Addr::new(
                    ReadBytesExt::read_u8(&mut rdr).unwrap(),
                    ReadBytesExt::read_u8(&mut rdr).unwrap(),
                    ReadBytesExt::read_u8(&mut rdr).unwrap(),
                    ReadBytesExt::read_u8(&mut rdr).unwrap(),
                );

                let port = ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap();

                Ok(Self::SocketAddress(SocketAddr::from((addr, port))))
            }
            Self::TYPE_IPV6 => {
                let mut buf = [0; 18];
                stream.read_exact(&mut buf).await?;
                let mut rdr = Cursor::new(buf);

                let addr = Ipv6Addr::new(
                    ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap(),
                    ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap(),
                    ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap(),
                    ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap(),
                    ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap(),
                    ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap(),
                    ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap(),
                    ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap(),
                );

                let port = ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap();

                Ok(Self::SocketAddress(SocketAddr::from((addr, port))))
            }
            _ => Err(Error::InvalidAddressType(addr_type)),
        }
    }

    pub async fn write_to<W>(&self, writer: &mut W) -> Result<(), Error>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = Vec::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        writer.write_all(&buf).await?;
        Ok(())
    }

    pub fn write_to_buf<B: BufMut>(&self, buf: &mut B) {
        match self {
            Self::DomainAddress(addr, port) => {
                buf.put_u8(Self::TYPE_DOMAIN);
                buf.put_u8(addr.len() as u8);
                buf.put_slice(addr.as_bytes());
                buf.put_u16(*port);
            }
            Self::SocketAddress(addr) => match addr {
                SocketAddr::V4(addr) => {
                    buf.put_u8(Self::TYPE_IPV4);
                    buf.put_slice(&addr.ip().octets());
                    buf.put_u16(addr.port());
                }
                SocketAddr::V6(addr) => {
                    buf.put_u8(Self::TYPE_IPV6);
                    for seg in addr.ip().segments() {
                        buf.put_u16(seg);
                    }
                    buf.put_u16(addr.port());
                }
            },
        }
    }

    pub fn serialized_len(&self) -> usize {
        1 + match self {
            Address::DomainAddress(addr, _) => 1 + addr.len() + 2,
            Address::SocketAddress(addr) => match addr {
                SocketAddr::V4(_) => 6,
                SocketAddr::V6(_) => 18,
            },
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error("unsupported TUIC version: {0:#x}")]
    UnsupportedVersion(u8),
    #[error("invalid response: {0:#x}")]
    InvalidResponse(u8),
    #[error("invalid command: {0:#x}")]
    InvalidCommand(u8),
    #[error("invalid address type: {0:#x}")]
    InvalidAddressType(u8),
    #[error("invalid address encoding: {0}")]
    InvalidAddressEncoding(#[from] FromUtf8Error),
}

impl From<Error> for IoError {
    fn from(err: Error) -> Self {
        let kind = match err {
            Error::Io(err) => return err,
            _ => IoErrorKind::Other,
        };

        Self::new(kind, err)
    }
}
