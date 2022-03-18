use super::{Address, Command, Error, SOCKS5_VERSION};
use tokio::io::{AsyncRead, AsyncReadExt};

/// Request
///
/// ```plain
/// +-----+-----+-------+------+----------+----------+
/// | VER | CMD |  RSV  | ATYP | DST.ADDR | DST.PORT |
/// +-----+-----+-------+------+----------+----------+
/// |  1  |  1  | X'00' |  1   | Variable |    2     |
/// +-----+-----+-------+------+----------+----------+
/// ```
#[derive(Clone)]
pub struct Request {
    pub command: Command,
    pub address: Address,
}

impl Request {
    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0; 3];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        if ver != SOCKS5_VERSION {
            return Err(Error::UnsupportedSocks5Version(ver));
        }

        let cmd = buf[1];
        let command = match Command::from_u8(cmd) {
            Some(c) => c,
            None => return Err(Error::UnsupportedCommand(cmd)),
        };

        let address = Address::read_from(r).await?;
        Ok(Self { command, address })
    }
}
