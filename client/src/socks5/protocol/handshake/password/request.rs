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

pub struct Request {
    pub username: Vec<u8>,
    pub password: Vec<u8>,
}

impl Request {
    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0; 2];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let ulen = buf[1];

        if ver != SUBNEGOTIATION_VERSION {
            return Err(Error::UnsupportedPasswordAuthenticationVersion(ver));
        }

        let mut buf = vec![0; ulen as usize + 1];
        r.read_exact(&mut buf).await?;

        let plen = buf[ulen as usize];
        buf.truncate(ulen as usize);
        let username = buf;

        let mut password = vec![0; plen as usize];
        r.read_exact(&mut password).await?;

        Ok(Self { username, password })
    }
}
