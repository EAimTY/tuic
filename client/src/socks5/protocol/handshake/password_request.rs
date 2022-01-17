use super::{Error, Socks5AuthMethod, SOCKS5_SUBNEGOTIATION_VERSION};
use tokio::io::{AsyncRead, AsyncReadExt};

/// SOCKS5 handshake password request packet
///
/// ```plain
/// +----+------+----------+------+----------+
/// |VER | ULEN |  UNAME   | PLEN |  PASSWD  |
/// +----+------+----------+------+----------+
/// | 1  |  1   | 1 to 255 |  1   | 1 to 255 |
/// +----+------+----------+------+----------+
/// ```

#[derive(Clone, Debug)]
pub struct HandshakePasswordRequest {
    pub uname: String,
    pub passwd: String,
}

impl HandshakePasswordRequest {
    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0u8; 1];
        r.read_exact(&mut buf).await?;
        let ver = buf[0];
        if ver != SOCKS5_SUBNEGOTIATION_VERSION {
            return Err(Error::UnsupportedVersion(ver));
        }

        r.read_exact(&mut buf).await?;
        let ulen = buf[0];

        let mut uname = vec![0u8; ulen as usize];
        r.read_exact(&mut uname).await?;

        r.read_exact(&mut buf).await?;
        let plen = buf[0];

        let mut passwd = vec![0u8; plen as usize];
        r.read_exact(&mut passwd).await?;

        Ok(Self {
            uname: String::from_utf8(uname).unwrap(),
            passwd: String::from_utf8(passwd).unwrap(),
        })
    }

    pub fn authenticated(&self, auth_method: &Socks5AuthMethod) -> bool {
        match auth_method {
            Socks5AuthMethod::PASSWORD { username, password } => {
                if username == &self.uname && password == &self.passwd {
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}
