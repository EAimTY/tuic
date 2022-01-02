#[derive(Clone, Copy, Debug)]
pub enum Command {
    Connect,
}

impl Command {
    const CMD_CONNECT: u8 = 0;

    #[inline]
    pub(crate) fn as_u8(self) -> u8 {
        match self {
            Self::Connect => Self::CMD_CONNECT,
        }
    }

    #[inline]
    pub(crate) fn from_u8(code: u8) -> Option<Self> {
        match code {
            Self::CMD_CONNECT => Some(Command::Connect),
            _ => None,
        }
    }
}
