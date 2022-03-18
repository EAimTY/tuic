use super::{HandshakeMethod, SOCKS5_VERSION};
use bytes::{BufMut, BytesMut};
use std::io::Result as IoResult;
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
#[derive(Clone)]
pub struct HandshakeResponse {
    pub method: HandshakeMethod,
}

impl HandshakeResponse {
    pub fn new(method: HandshakeMethod) -> Self {
        Self { method }
    }

    pub async fn write_to<W>(&self, w: &mut W) -> IoResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = BytesMut::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        w.write_all(&buf).await
    }

    pub fn write_to_buf<B: BufMut>(&self, buf: &mut B) {
        buf.put_u8(SOCKS5_VERSION);
        buf.put_u8(self.method.as_u8());
    }

    pub fn serialized_len(&self) -> usize {
        2
    }
}
