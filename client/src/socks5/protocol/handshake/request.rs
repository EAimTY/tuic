use super::{Error, HandshakeMethod, SOCKS5_VERSION};
use std::mem::ManuallyDrop;
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
#[derive(Clone)]
pub struct HandshakeRequest {
    pub methods: Vec<HandshakeMethod>,
}

impl HandshakeRequest {
    pub async fn read_from<R>(r: &mut R) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = [0; 2];
        r.read_exact(&mut buf).await?;

        let ver = buf[0];
        let mlen = buf[1];

        if ver != SOCKS5_VERSION {
            return Err(Error::UnsupportedSocks5Version(ver));
        }

        let methods = unsafe {
            let mut methods = vec![0; mlen as usize];
            r.read_exact(&mut methods).await?;

            let mut methods = ManuallyDrop::new(methods);

            Vec::from_raw_parts(
                methods.as_mut_ptr() as *mut HandshakeMethod,
                methods.len(),
                methods.capacity(),
            )
        };

        Ok(Self { methods })
    }
}
