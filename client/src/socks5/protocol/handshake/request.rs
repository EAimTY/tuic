use super::{Error, SOCKS5_VERSION};
use tokio::io::{AsyncRead, AsyncReadExt};

/// SOCKS5 handshake request packet
///
/// ```plain
/// +-----+----------+----------+
/// | VER | NMETHODS | METHODS  |
/// +-----+----------+----------+
/// |  1  |    1     | 1 to 255 |
/// +-----+----------+----------|
/// ```
#[derive(Clone, Debug)]
pub struct HandshakeRequest {
    pub methods: Vec<u8>,
}

impl HandshakeRequest {
    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let nmet = buf[1];

        if ver != SOCKS5_VERSION {
            return Err(Error::UnsupportedSocks5Version(ver));
        }

        let mut methods = vec![0u8; nmet as usize];
        r.read_exact(&mut methods).await?;

        Ok(Self { methods })
    }
}
