use super::SOCKS5_VERSION;
use bytes::{BufMut, BytesMut};
use std::io;
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// SOCKS5 handshake response packet
///
/// ```plain
/// +-----+--------+
/// | VER | METHOD |
/// +-----+--------+
/// |  1  |   1    |
/// +-----+--------+
/// ```
#[derive(Clone, Debug)]
pub struct HandshakeResponse {
    pub chosen_method: u8,
}

impl HandshakeResponse {
    pub fn new(cm: u8) -> Self {
        Self { chosen_method: cm }
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
        buf.put_u8(self.chosen_method);
    }

    pub fn serialized_len(&self) -> usize {
        2
    }
}
