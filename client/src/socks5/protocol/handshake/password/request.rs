use super::{Error, SUBNEGOTIATION_VERSION};
use tokio::io::{AsyncRead, AsyncReadExt};

/// SOCKS5 password handshake request packet
///
/// ```plain
/// +-----+------+----------+------+----------+
/// | VER | ULEN |  UNAME   | PLEN |  PASSWD  |
/// +-----+------+----------+------+----------+
/// |  1  |  1   | 1 to 255 |  1   | 1 to 255 |
/// +-----+------+----------+------+----------+
/// ```

#[derive(Debug)]
pub struct Request {
    pub username: Vec<u8>,
    pub password: Vec<u8>,
}

impl Request {
    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let ulen = buf[1];

        if ver != SUBNEGOTIATION_VERSION {
            return Err(Error::UnsupportedPasswordAuthenticationVersion(ver));
        }

        let mut username = vec![0; ulen as usize];
        r.read_exact(&mut username).await?;

        let plen = r.read_u8().await?;
        let mut password = vec![0; plen as usize];
        r.read_exact(&mut password).await?;

        Ok(Self { username, password })
    }
}
