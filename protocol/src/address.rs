use crate::Error;
use bytes::{BufMut, BytesMut};
use std::{
    io::Result as IoResult,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    vec::IntoIter,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Address
///
/// ```plain
/// +------+----------+----------+
/// | TYPE |   ADDR   |   PORT   |
/// +------+----------+----------+
/// |  1   | Variable |    2     |
/// +------+----------+----------+
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Address {
    HostnameAddress(String, u16),
    SocketAddress(SocketAddr),
}

impl Address {
    const TYPE_HOSTNAME: u8 = 0x00;
    const TYPE_IPV4: u8 = 0x01;
    const TYPE_IPV6: u8 = 0x02;

    pub async fn read_from<R>(stream: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let addr_type = stream.read_u8().await?;

        match addr_type {
            Self::TYPE_HOSTNAME => {
                let len = stream.read_u8().await? as usize;

                let mut buf = vec![0; len + 2];
                stream.read_exact(&mut buf).await?;

                let port = unsafe { u16::from_be(*(buf.as_ptr().add(len) as *const u16)) };

                buf.truncate(len);

                let addr = String::from_utf8(buf).map_err(|_| Error::AddressInvalidEncoding)?;

                Ok(Self::HostnameAddress(addr, port))
            }
            Self::TYPE_IPV4 => {
                let mut buf = [0; 6];
                stream.read_exact(&mut buf).await?;

                let port = unsafe { u16::from_be(*(buf.as_ptr().add(4) as *const u16)) };
                let addr = Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);

                Ok(Self::SocketAddress(SocketAddr::from((addr, port))))
            }
            Self::TYPE_IPV6 => {
                let mut buf = [0; 18];
                stream.read_exact(&mut buf).await?;
                let buf = unsafe { *(buf.as_ptr() as *const [u16; 9]) };

                let port = buf[8];

                let addr = Ipv6Addr::new(
                    u16::from_be(buf[0]),
                    u16::from_be(buf[1]),
                    u16::from_be(buf[2]),
                    u16::from_be(buf[3]),
                    u16::from_be(buf[4]),
                    u16::from_be(buf[5]),
                    u16::from_be(buf[6]),
                    u16::from_be(buf[7]),
                );

                Ok(Self::SocketAddress(SocketAddr::from((addr, port))))
            }
            _ => Err(Error::UnsupportedAddressType(addr_type)),
        }
    }

    pub async fn write_to<W>(&self, writer: &mut W) -> IoResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = BytesMut::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        writer.write_all(&buf).await
    }

    pub fn write_to_buf<B: BufMut>(&self, buf: &mut B) {
        match self {
            Self::HostnameAddress(addr, port) => {
                assert!(addr.len() < u8::MAX as usize);

                buf.put_u8(Self::TYPE_HOSTNAME);
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
            Address::HostnameAddress(addr, _) => 1 + addr.len() + 2,
            Address::SocketAddress(addr) => match addr {
                SocketAddr::V4(_) => 6,
                SocketAddr::V6(_) => 18,
            },
        }
    }
}

impl ToSocketAddrs for Address {
    type Iter = IntoIter<SocketAddr>;

    fn to_socket_addrs(&self) -> IoResult<Self::Iter> {
        Ok(match self {
            Self::HostnameAddress(addr, port) => (addr.as_str(), *port).to_socket_addrs()?,
            Self::SocketAddress(addr) => vec![addr.to_owned()].into_iter(),
        })
    }
}
