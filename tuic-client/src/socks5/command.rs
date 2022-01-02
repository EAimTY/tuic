pub const SOCKS5_CMD_CONNECT: u8 = 0x01;

#[derive(Clone, Copy, Debug)]
pub enum Command {
    Connect,
}

impl Command {
    #[inline]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Connect => SOCKS5_CMD_CONNECT,
        }
    }

    #[inline]
    pub fn from_u8(code: u8) -> Option<Self> {
        match code {
            SOCKS5_CMD_CONNECT => Some(Command::Connect),
            _ => None,
        }
    }
}
