use crate::Error;
use bytes::{BufMut, BytesMut};
use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    slice,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Clone, Debug, Copy)]
pub struct Address {
    socket_addr: SocketAddr,
}

impl Address {
    const ATYP_IPV4: u8 = 0;
    const ATYP_IPV6: u8 = 1;

    /// Parse from a `AsyncRead`
    pub async fn read_from<R>(stream: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut addr_type_buf = [0u8; 1];
        stream.read_exact(&mut addr_type_buf).await?;

        let addr_type = addr_type_buf[0];
        match addr_type {
            Self::ATYP_IPV4 => {
                let mut buf = [0u8; 6];
                stream.read_exact(&mut buf).await?;

                let v4addr = Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);
                let port = unsafe {
                    let raw_port = &buf[4..];
                    u16::from_be(*(raw_port.as_ptr() as *const _))
                };

                Ok(Self {
                    socket_addr: SocketAddr::new(IpAddr::V4(v4addr), port),
                })
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

                Ok(Self {
                    socket_addr: SocketAddr::new(IpAddr::V6(v6addr), port),
                })
            }
            _ => Err(Error::AddressTypeNotSupported(addr_type)),
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
        match self.socket_addr {
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
        }
    }

    #[inline]
    pub fn serialized_len(&self) -> usize {
        match self.socket_addr {
            SocketAddr::V4(_) => 1 + 4 + 2,
            SocketAddr::V6(_) => 1 + 8 * 2 + 2,
        }
    }

    #[inline]
    pub fn ip(&self) -> IpAddr {
        self.socket_addr.ip()
    }

    #[inline]
    pub fn port(&self) -> u16 {
        self.socket_addr.port()
    }
}
