use super::SUBNEGOTIATION_VERSION;
use bytes::{BufMut, BytesMut};
use std::io::Result as IoResult;
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// SOCKS5 password handshake response
///
/// ```plain
/// +-----+--------+
/// | VER | STATUS |
/// +-----+--------+
/// |  1  |   1    |
/// +-----+--------+
/// ```

#[derive(Clone)]
pub struct Response(bool);

impl Response {
    const STATUS_SUCCEEDED: u8 = 0x00;
    const STATUS_FAILED: u8 = 0xff;

    pub fn new(is_succeed: bool) -> Self {
        Self(is_succeed)
    }

    pub fn is_succeed(&self) -> bool {
        self.0
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
        buf.put_u8(SUBNEGOTIATION_VERSION);

        if self.is_succeed() {
            buf.put_u8(Self::STATUS_SUCCEEDED);
        } else {
            buf.put_u8(Self::STATUS_FAILED);
        }
    }

    pub fn serialized_len(&self) -> usize {
        2
    }
}
