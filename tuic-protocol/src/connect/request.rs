use crate::{Address, Command, Error, TUIC_PROTOCOL_VERSION};
use bytes::{BufMut, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// CONNECT request
///
/// ```plain
/// +----+-----+------+----------+----------+
/// |VER | CMD | ATYP | DST.ADDR | DST.PORT |
/// +----+-----+------+----------+----------+
/// | 1  |  1  |  1   | Variable |    2     |
/// +----+-----+------+----------+----------+
/// ```
///
/// ATYP:
/// - 0x01: IPv4 address
/// - 0x03: Domain name
/// - 0x04: IPv6 address
#[derive(Clone, Debug)]
pub struct ConnectRequest {
    pub command: Command,
    pub address: Address,
}

impl ConnectRequest {
    pub fn new(command: Command, address: Address) -> Self {
        Self { command, address }
    }

    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0u8; 3];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        if ver != TUIC_PROTOCOL_VERSION {
            return Err(Error::UnsupportedTuicVersion(ver));
        }

        let cmd = buf[1];
        let command = match Command::from_u8(cmd) {
            Some(c) => c,
            None => return Err(Error::UnsupportedCommand(cmd)),
        };

        let address = Address::read_from(r).await?;
        Ok(Self { command, address })
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
        self.address.write_to_buf(buf);
    }

    #[inline]
    pub fn serialized_len(&self) -> usize {
        self.address.serialized_len() + 2
    }
}
