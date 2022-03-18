use super::{Address, Reply, SOCKS5_VERSION};
use bytes::{BufMut, BytesMut};
use std::io::Result as IoResult;
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// Response
///
/// ```plain
/// +-----+-----+-------+------+----------+----------+
/// | VER | REP |  RSV  | ATYP | BND.ADDR | BND.PORT |
/// +-----+-----+-------+------+----------+----------+
/// |  1  |  1  | X'00' |  1   | Variable |    2     |
/// +-----+-----+-------+------+----------+----------+
/// ```
#[derive(Clone)]
pub struct Response {
    pub reply: Reply,
    pub address: Address,
}

impl Response {
    pub fn new(reply: Reply, address: Address) -> Self {
        Self { reply, address }
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
        buf.put_u8(self.reply.as_u8());
        buf.put_u8(0x00);
        self.address.write_to_buf(buf);
    }

    pub fn serialized_len(&self) -> usize {
        3 + self.address.serialized_len()
    }
}
