#[derive(Clone, Copy, Debug)]
pub enum Command {
    Connect,
}

impl Command {
    const CMD_CONNECT: u8 = 0x01;

    #[inline]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Connect => Self::CMD_CONNECT,
        }
    }

    #[inline]
    pub fn from_u8(code: u8) -> Option<Self> {
        match code {
            Self::CMD_CONNECT => Some(Command::Connect),
            _ => None,
        }
    }
}
