use crate::Error;
use bytes::{BufMut, BytesMut};
use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    slice, vec,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Address {
    SocketAddress(SocketAddr),
    UriAuthorityAddress(String, u16),
}

impl Address {
    const ATYP_IPV4: u8 = 1;
    const ATYP_URI_AUTHORITY: u8 = 3;
    const ATYP_IPV6: u8 = 4;

    pub async fn read_from<R>(stream: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut atyp_buf = [0u8; 1];
        stream.read_exact(&mut atyp_buf).await?;

        let atyp = atyp_buf[0];
        match atyp {
            Self::ATYP_IPV4 => {
                let mut buf = [0u8; 6];
                stream.read_exact(&mut buf).await?;

                let v4addr = Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);
                let port = unsafe {
                    let raw_port = &buf[4..];
                    u16::from_be(*(raw_port.as_ptr() as *const _))
                };

                Ok(Self::SocketAddress(SocketAddr::new(
                    IpAddr::V4(v4addr),
                    port,
                )))
            }
            Self::ATYP_IPV6 => {
                let mut buf = [0u8; 18];
                stream.read_exact(&mut buf).await?;

                let buf: &[u16] = unsafe { slice::from_raw_parts(buf.as_ptr() as *const _, 9) };

                let v6addr = Ipv6Addr::new(
                    u16::from_be(buf[0]),
                    u16::from_be(buf[1]),
                    u16::from_be(buf[2]),
                    u16::from_be(buf[3]),
                    u16::from_be(buf[4]),
                    u16::from_be(buf[5]),
                    u16::from_be(buf[6]),
                    u16::from_be(buf[7]),
                );
                let port = u16::from_be(buf[8]);

                Ok(Self::SocketAddress(SocketAddr::new(
                    IpAddr::V6(v6addr),
                    port,
                )))
            }
            Self::ATYP_URI_AUTHORITY => {
                let mut length_buf = [0u8; 1];
                stream.read_exact(&mut length_buf).await?;
                let length = length_buf[0] as usize;

                let buf_length = length + 2;

                let mut raw_addr = vec![0u8; buf_length];
                stream.read_exact(&mut raw_addr).await?;

                let raw_port = &raw_addr[length..];
                let port = unsafe { u16::from_be(*(raw_port.as_ptr() as *const _)) };

                raw_addr.truncate(length);

                let addr = match String::from_utf8(raw_addr) {
                    Ok(addr) => addr,
                    Err(..) => return Err(Error::AddressDomainInvalidEncoding),
                };

                Ok(Self::UriAuthorityAddress(addr, port))
            }
            _ => Err(Error::AddressTypeNotSupported(atyp)),
        }
    }

    #[inline]
    pub async fn write_to<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = BytesMut::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        writer.write_all(&buf).await
    }

    #[inline]
    pub fn write_to_buf<B: BufMut>(&self, buf: &mut B) {
        match self {
            Self::SocketAddress(addr) => match addr {
                SocketAddr::V4(addr) => {
                    buf.put_u8(Self::ATYP_IPV4);
                    buf.put_slice(&addr.ip().octets());
                    buf.put_u16(addr.port());
                }
                SocketAddr::V6(addr) => {
                    buf.put_u8(Self::ATYP_IPV6);
                    for seg in &addr.ip().segments() {
                        buf.put_u16(*seg);
                    }
                    buf.put_u16(addr.port());
                }
            },
            Self::UriAuthorityAddress(addr, port) => {
                if addr.len() <= u8::MAX as usize {
                    panic!("domain name length must be smaller than 256");
                }

                buf.put_u8(Self::ATYP_URI_AUTHORITY);
                buf.put_u8(addr.len() as u8);
                buf.put_slice(addr.as_bytes());
                buf.put_u16(*port);
            }
        }
    }

    #[inline]
    pub fn serialized_len(&self) -> usize {
        match self {
            Address::SocketAddress(addr) => match addr {
                SocketAddr::V4(_) => 1 + 4 + 2,
                SocketAddr::V6(_) => 1 + 8 * 2 + 2,
            },
            Address::UriAuthorityAddress(addr, _) => 1 + 1 + addr.len() + 2,
        }
    }

    #[inline]
    pub fn max_serialized_len() -> usize {
        1 + 1 + u8::MAX as usize + 2
    }

    pub fn port(&self) -> u16 {
        match self {
            Address::SocketAddress(addr) => addr.port(),
            Address::UriAuthorityAddress(.., port) => *port,
        }
    }

    pub fn host(&self) -> String {
        match self {
            Address::SocketAddress(addr) => addr.ip().to_string(),
            Address::UriAuthorityAddress(domain, ..) => domain.to_owned(),
        }
    }
}

impl From<SocketAddr> for Address {
    fn from(s: SocketAddr) -> Address {
        Address::SocketAddress(s)
    }
}

impl From<(String, u16)> for Address {
    fn from((authority, port): (String, u16)) -> Address {
        Address::UriAuthorityAddress(authority, port)
    }
}

impl ToSocketAddrs for Address {
    type Iter = vec::IntoIter<SocketAddr>;

    fn to_socket_addrs(&self) -> io::Result<vec::IntoIter<SocketAddr>> {
        match self.clone() {
            Address::SocketAddress(addr) => Ok(vec![addr].into_iter()),
            Address::UriAuthorityAddress(addr, port) => (&addr[..], port).to_socket_addrs(),
        }
    }
}
