use super::{Address, Error, Reply, SOCKS5_VERSION};
use bytes::{BufMut, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Response
///
/// ```plain
/// +-----+-----+-------+------+----------+----------+
/// | VER | REP |  RSV  | ATYP | BND.ADDR | BND.PORT |
/// +-----+-----+-------+------+----------+----------+
/// |  1  |  1  | X'00' |  1   | Variable |    2     |
/// +-----+-----+-------+------+----------+----------+
/// ```
#[derive(Clone, Debug)]
pub struct Response {
    pub reply: Reply,
    pub address: Address,
}

impl Response {
    pub fn new(reply: Reply, address: Address) -> Self {
        Self { reply, address }
    }

    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0u8; 3];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let reply_code = buf[1];

        if ver != SOCKS5_VERSION {
            return Err(Error::UnsupportedVersion(ver));
        }

        let address = Address::read_from(r).await?;

        Ok(Self {
            reply: Reply::from_u8(reply_code),
            address,
        })
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
        buf.put_u8(self.reply.as_u8());
        buf.put_u8(0x00);
        self.address.write_to_buf(buf);
    }

    #[inline]
    pub fn serialized_len(&self) -> usize {
        self.address.serialized_len() + 3
    }
}
