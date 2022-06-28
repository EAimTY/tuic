//! The TUIC protocol

use byteorder::{BigEndian, ReadBytesExt};
use bytes::BufMut;
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    io::{Cursor, Error, ErrorKind, Result},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const TUIC_PROTOCOL_VERSION: u8 = 0x04;

/// Command
///
/// ```plain
/// +-----+------+----------+
/// | VER | TYPE |   OPT    |
/// +-----+------+----------+
/// |  1  |  1   | Variable |
/// +-----+------+----------+
/// ```
#[non_exhaustive]
#[derive(Clone)]
pub enum Command {
    Response(bool),
    Authenticate {
        digest: [u8; 32],
    },
    Connect {
        addr: Address,
    },
    Packet {
        assoc_id: u32,
        len: u16,
        addr: Address,
    },
    Dissociate {
        assoc_id: u32,
    },
    Heartbeat,
}

impl Command {
    const TYPE_RESPONSE: u8 = 0xff;
    const TYPE_AUTHENTICATE: u8 = 0x00;
    const TYPE_CONNECT: u8 = 0x01;
    const TYPE_PACKET: u8 = 0x02;
    const TYPE_DISSOCIATE: u8 = 0x03;
    const TYPE_HEARTBEAT: u8 = 0x04;

    const RESPONSE_SUCCEEDED: u8 = 0x00;
    const RESPONSE_FAILED: u8 = 0xff;

    pub fn new_response(is_succeeded: bool) -> Self {
        Self::Response(is_succeeded)
    }

    pub fn new_authenticate(digest: [u8; 32]) -> Self {
        Self::Authenticate { digest }
    }

    pub fn new_connect(addr: Address) -> Self {
        Self::Connect { addr }
    }

    pub fn new_packet(assoc_id: u32, len: u16, addr: Address) -> Self {
        Self::Packet {
            assoc_id,
            len,
            addr,
        }
    }

    pub fn new_dissociate(assoc_id: u32) -> Self {
        Self::Dissociate { assoc_id }
    }

    pub fn new_heartbeat() -> Self {
        Self::Heartbeat
    }

    pub async fn read_from<R>(r: &mut R) -> Result<Self>
    where
        R: AsyncRead + Unpin,
    {
        let ver = r.read_u8().await?;

        if ver != TUIC_PROTOCOL_VERSION {
            return Err(Error::new(
                ErrorKind::Unsupported,
                format!("Unsupported TUIC version: {ver}"),
            ));
        }

        let cmd = r.read_u8().await?;
        match cmd {
            Self::TYPE_RESPONSE => {
                let resp = r.read_u8().await?;
                match resp {
                    Self::RESPONSE_SUCCEEDED => Ok(Self::new_response(true)),
                    Self::RESPONSE_FAILED => Ok(Self::new_response(false)),
                    _ => Err(Error::new(
                        ErrorKind::InvalidInput,
                        format!("Invalid response code: {resp}"),
                    )),
                }
            }
            Self::TYPE_AUTHENTICATE => {
                let mut digest = [0; 32];
                r.read_exact(&mut digest).await?;
                Ok(Self::new_authenticate(digest))
            }
            Self::TYPE_CONNECT => {
                let addr = Address::read_from(r).await?;
                Ok(Self::new_connect(addr))
            }
            Self::TYPE_PACKET => {
                let mut buf = [0; 6];
                r.read_exact(&mut buf).await?;
                let mut rdr = Cursor::new(buf);

                let assoc_id = ReadBytesExt::read_u32::<BigEndian>(&mut rdr).unwrap();
                let len = ReadBytesExt::read_u16::<BigEndian>(&mut rdr).unwrap();
                let addr = Address::read_from(r).await?;

                Ok(Self::new_packet(assoc_id, len, addr))
            }
            Self::TYPE_DISSOCIATE => {
                let assoc_id = r.read_u32().await?;
                Ok(Self::new_dissociate(assoc_id))
            }
            Self::TYPE_HEARTBEAT => Ok(Self::new_heartbeat()),
            _ => Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Invalid command: {cmd}"),
            )),
        }
    }

    pub async fn write_to<W>(&self, w: &mut W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = Vec::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        w.write_all(&buf).await
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
            Self::Authenticate { digest } => {
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

    pub const fn max_serialized_len() -> usize {
        2 + 6 + Address::max_serialized_len()
    }
}

/// Address
///
/// ```plain
/// +------+----------+----------+
/// | TYPE |   ADDR   |   PORT   |
/// +------+----------+----------+
/// |  1   | Variable |    2     |
/// +------+----------+----------+
/// ```
///
/// The address type can be one of the following:
/// 0x00: fully-qualified domain name (the first byte indicates the length of the domain name)
/// 0x01: IPv4 address
/// 0x02: IPv6 address
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Address {
    DomainAddress(String, u16),
    SocketAddress(SocketAddr),
}

impl Address {
    const TYPE_DOMAIN: u8 = 0x00;
    const TYPE_IPV4: u8 = 0x01;
    const TYPE_IPV6: u8 = 0x02;

    pub async fn read_from<R>(stream: &mut R) -> Result<Self>
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

                let addr = String::from_utf8(buf).map_err(|err| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Invalid address encoding: {err}"),
                    )
                })?;

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
            _ => Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Unsupported address type: {addr_type}"),
            )),
        }
    }

    pub async fn write_to<W>(&self, writer: &mut W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = Vec::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        writer.write_all(&buf).await
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

    pub const fn max_serialized_len() -> usize {
        1 + 1 + u8::MAX as usize + 2
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::DomainAddress(addr, port) => write!(f, "{addr}:{port}"),
            Self::SocketAddress(addr) => write!(f, "{addr}"),
        }
    }
}
