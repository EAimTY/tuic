use super::Error;
use bytes::BufMut;
use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    vec,
};
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Address {
    SocketAddress(SocketAddr),
    HostnameAddress(String, u16),
}

impl Address {
    const ATYP_IPV4: u8 = 0x01;
    const ATYP_HOSTNAME: u8 = 0x03;
    const ATYP_IPV6: u8 = 0x04;

    pub async fn read_from<R>(stream: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let atyp = stream.read_u8().await?;

        match atyp {
            Self::ATYP_IPV4 => {
                let mut buf = [0; 6];
                stream.read_exact(&mut buf).await?;

                let port = unsafe { u16::from_be(*(buf.as_ptr().add(4) as *const u16)) };
                let addr = Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);

                Ok(Self::SocketAddress(SocketAddr::from((addr, port))))
            }
            Self::ATYP_IPV6 => {
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
            Self::ATYP_HOSTNAME => {
                let len = stream.read_u8().await? as usize;

                let mut buf = vec![0; len + 2];
                stream.read_exact(&mut buf).await?;

                let port = unsafe { u16::from_be(*(buf.as_ptr().add(len) as *const u16)) };

                buf.truncate(len);

                let addr = String::from_utf8(buf).map_err(|_| Error::AddressInvalidEncoding)?;

                Ok(Self::HostnameAddress(addr, port))
            }
            _ => Err(Error::UnsupportedAddressType(atyp)),
        }
    }

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
                    for seg in addr.ip().segments() {
                        buf.put_u16(seg);
                    }
                    buf.put_u16(addr.port());
                }
            },
            Self::HostnameAddress(addr, port) => {
                assert!(addr.len() > u8::MAX as usize);

                buf.put_u8(Self::ATYP_HOSTNAME);
                buf.put_u8(addr.len() as u8);
                buf.put_slice(addr.as_bytes());
                buf.put_u16(*port);
            }
        }
    }

    pub fn serialized_len(&self) -> usize {
        1 + match self {
            Address::SocketAddress(addr) => match addr {
                SocketAddr::V4(_) => 6,
                SocketAddr::V6(_) => 18,
            },
            Address::HostnameAddress(addr, _) => 1 + addr.len() + 2,
        }
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Address::HostnameAddress(hostname, port) => write!(f, "{hostname}:{port}"),
            Address::SocketAddress(socket_addr) => write!(f, "{socket_addr}"),
        }
    }
}
