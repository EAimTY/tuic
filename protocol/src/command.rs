#[derive(Clone, Copy, Debug)]
pub enum Command {
    Connect,
    Associate,
}

impl Command {
    const CMD_CONNECT: u8 = 0x01;
    const CMD_ASSOCIATE: u8 = 0x03;

    #[inline]
    pub(crate) fn as_u8(self) -> u8 {
        match self {
            Self::Connect => Self::CMD_CONNECT,
            Self::Associate => Self::CMD_ASSOCIATE,
        }
    }

    #[inline]
    pub(crate) fn from_u8(code: u8) -> Option<Self> {
        match code {
            Self::CMD_CONNECT => Some(Command::Connect),
            Self::CMD_ASSOCIATE => Some(Command::Associate),
            _ => None,
        }
    }
}
