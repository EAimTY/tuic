#[derive(Clone, Copy, Debug)]
pub enum Command {
    Connect,
    Bind,
    Associate,
}

impl Command {
    const CMD_CONNECT: u8 = 0x01;
    const CMD_BIND: u8 = 0x02;
    const CMD_ASSOCIATE: u8 = 0x03;

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Connect => Self::CMD_CONNECT,
            Self::Bind => Self::CMD_BIND,
            Self::Associate => Self::CMD_ASSOCIATE,
        }
    }

    pub fn from_u8(code: u8) -> Option<Self> {
        match code {
            Self::CMD_CONNECT => Some(Command::Connect),
            Self::CMD_BIND => Some(Command::Bind),
            Self::CMD_ASSOCIATE => Some(Command::Associate),
            _ => None,
        }
    }
}
