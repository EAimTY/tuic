use super::{Error, SOCKS5_VERSION};
use bytes::{BufMut, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// SOCKS5 handshake request packet
///
/// ```plain
/// +-----+----------+----------+
/// | VER | NMETHODS | METHODS  |
/// +-----+----------+----------+
/// |  5  |    1     | 1 to 255 |
/// +-----+----------+----------|
/// ```
#[derive(Clone, Debug)]
pub struct HandshakeRequest {
    pub methods: Vec<u8>,
}

impl HandshakeRequest {
    pub fn new(methods: Vec<u8>) -> Self {
        Self { methods }
    }

    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let nmet = buf[1];

        if ver != SOCKS5_VERSION {
            return Err(Error::UnsupportedVersion(ver));
        }

        let mut methods = vec![0u8; nmet as usize];
        r.read_exact(&mut methods).await?;

        Ok(Self { methods })
    }

    pub async fn write_to<W>(&self, w: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = BytesMut::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        w.write_all(&buf).await
    }

    pub fn write_to_buf<B: BufMut>(&self, buf: &mut B) {
        buf.put_u8(SOCKS5_VERSION);
        buf.put_u8(self.methods.len() as u8);
        buf.put_slice(&self.methods);
    }

    pub fn serialized_len(&self) -> usize {
        2 + self.methods.len()
    }
}
