use super::SOCKS5_SUBNEGOTIATION_VERSION;
use bytes::{BufMut, BytesMut};
use std::io;
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// SOCKS5 handshake password response packet
///
/// ```plain
/// +-----+--------+
/// | VER | STATUS |
/// +-----+--------+
/// |  1  |   1    |
/// +-----+--------+
/// ```

#[derive(Clone, Debug)]
pub struct HandshakePasswordResponse {
    pub status: u8,
}

impl HandshakePasswordResponse {
    pub fn new(status: u8) -> Self {
        Self { status }
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
        buf.put_u8(SOCKS5_SUBNEGOTIATION_VERSION);
        buf.put_u8(self.status);
    }

    pub fn serialized_len(&self) -> usize {
        2
    }
}
