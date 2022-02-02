use crate::{Address, Command, Error, TUIC_PROTOCOL_VERSION};
use bytes::{BufMut, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Request
///
/// ```plain
/// +-----+-----+-----+------+----------+----------+
/// | VER | CMD | TKN | ATYP | DST.ADDR | DST.PORT |
/// +-----+-----+-----+------+----------+----------+
/// |  1  |  1  |  8  |  1   | Variable |    2     |
/// +-----+-----+-----+------+----------+----------+
/// ```
#[derive(Clone, Debug)]
pub struct Request {
    pub token: u64,
    pub command: Command,
    pub address: Address,
}

impl Request {
    pub fn new(cmd: Command, token: u64, addr: Address) -> Self {
        Self {
            token,
            command: cmd,
            address: addr,
        }
    }

    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let cmd = buf[1];

        if ver != TUIC_PROTOCOL_VERSION {
            return Err(Error::UnsupportedVersion(ver));
        }

        let command = match Command::from_u8(cmd) {
            Some(c) => c,
            None => return Err(Error::UnsupportedCommand(cmd)),
        };

        let token = r.read_u64().await?;

        let address = Address::read_from(r).await?;

        Ok(Self {
            token,
            command,
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
        buf.put_u8(TUIC_PROTOCOL_VERSION);
        buf.put_u8(self.command.as_u8());
        buf.put_u64(self.token);
        self.address.write_to_buf(buf);
    }

    #[inline]
    pub fn serialized_len(&self) -> usize {
        1 + 1 + 8 + self.address.serialized_len()
    }
}
