use crate::{Error, TUIC_PROTOCOL_VERSION};
use bytes::{BufMut, BytesMut};
use std::io::Result as IoResult;
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
pub struct Response(bool);

impl Response {
    const REPLY_SUCCEEDED: u8 = 0x00;
    const REPLY_FAILED: u8 = 0xFF;

    pub fn new(reply: bool) -> Self {
        Self(reply)
    }

    pub fn is_succeeded(&self) -> bool {
        self.0
    }

    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0; 2];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let rep = buf[1];

        if ver != TUIC_PROTOCOL_VERSION {
            return Err(Error::UnsupportedVersion(ver));
        }

        match rep {
            Self::REPLY_SUCCEEDED => Ok(Self(true)),
            Self::REPLY_FAILED => Ok(Self(false)),
            _ => Err(Error::UnsupportedReply(rep)),
        }
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
        buf.put_u8(TUIC_PROTOCOL_VERSION);

        if self.is_succeeded() {
            buf.put_u8(Self::REPLY_SUCCEEDED);
        } else {
            buf.put_u8(Self::REPLY_FAILED);
        }
    }

    pub fn serialized_len(&self) -> usize {
        2
    }
}
