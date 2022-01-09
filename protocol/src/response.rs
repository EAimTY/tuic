use crate::{Error, Reply, TUIC_PROTOCOL_VERSION};
use bytes::{BufMut, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Response
///
/// ```plain
/// +-----+-----+
/// | VER | REP |
/// +-----+-----+
/// |  1  |  1  |
/// +-----+-----+
/// ```
#[derive(Clone, Debug)]
pub struct Response {
    pub reply: Reply,
}

impl Response {
    pub fn new(reply: Reply) -> Self {
        Self { reply }
    }

    pub fn is_succeeded(&self) -> bool {
        self.reply == Reply::Succeeded
    }

    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let rep = buf[1];

        let reply = Reply::from_u8(rep);

        if ver != TUIC_PROTOCOL_VERSION {
            Err(Error::UnsupportedVersion(ver))
        } else {
            Ok(Self { reply })
        }
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
        buf.put_u8(TUIC_PROTOCOL_VERSION);
        buf.put_u8(self.reply.as_u8());
    }

    pub fn serialized_len(&self) -> usize {
        1 + 1
    }
}
