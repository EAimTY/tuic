use crate::Error;
use bytes::{BufMut, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

/// Handshake request
#[derive(Clone, Debug)]
pub struct HandshakeRequest {
    pub token: Vec<u8>,
}

impl HandshakeRequest {
    pub fn new(token: Vec<u8>) -> Self {
        Self { token }
    }

    pub async fn read_from<R>(_r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        Ok(Self { token: Vec::new() })
    }

    pub async fn write_to<W>(&self, w: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = BytesMut::with_capacity(self.serialized_len());
        self.write_to_buf(&mut buf);
        w.write_all(&buf).await
    }

    pub fn write_to_buf<B: BufMut>(&self, _buf: &mut B) {}

    #[inline]
    pub fn serialized_len(&self) -> usize {
        0
    }
}
