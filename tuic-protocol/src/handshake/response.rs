use crate::{Error, TUIC_PROTOCOL_VERSION};
use bytes::{BufMut, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Handshake response
///
/// ```plain
/// +-----+-----+
/// | VER | REP |
/// +-----+-----+
/// |  1  |  1  |
/// +-----+-----+
/// ```
#[derive(Clone, Debug)]
pub struct HandshakeResponse {
    is_succeeded: bool,
}

impl HandshakeResponse {
    const HANDSHAKE_SUCCEEDED: u8 = 0;
    const HANDSHAKE_FAILED: u8 = 1;

    pub fn new(is_succeeded: bool) -> Self {
        Self { is_succeeded }
    }

    pub fn is_succeeded(&self) -> bool {
        self.is_succeeded
    }

    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let rep = buf[1];

        if ver != TUIC_PROTOCOL_VERSION {
            return Err(Error::UnsupportedTuicVersion(ver));
        }

        match rep {
            Self::HANDSHAKE_SUCCEEDED => Ok(Self { is_succeeded: true }),
            Self::HANDSHAKE_FAILED => Ok(Self {
                is_succeeded: false,
            }),
            _ => Err(Error::InvalidHandshakeResponse(rep)),
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

        if self.is_succeeded {
            buf.put_u8(Self::HANDSHAKE_SUCCEEDED);
        } else {
            buf.put_u8(Self::HANDSHAKE_FAILED);
        }
    }

    #[inline]
    pub fn serialized_len(&self) -> usize {
        2
    }
}
