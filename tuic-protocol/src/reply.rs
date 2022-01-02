#[derive(Clone, Copy, Debug)]
pub enum Reply {
    Succeeded,
    Failed,
    Other(u8),
}

impl Reply {
    const REPLY_SUCCEEDED: u8 = 0;
    const REPLY_FAILED: u8 = 1;

    #[inline]
    pub(crate) fn as_u8(self) -> u8 {
        match self {
            Self::Succeeded => Self::REPLY_SUCCEEDED,
            Self::Failed => Self::REPLY_FAILED,
            Self::Other(code) => code,
        }
    }

    #[inline]
    pub(crate) fn from_u8(code: u8) -> Self {
        match code {
            Self::REPLY_SUCCEEDED => Self::Succeeded,
            Self::REPLY_FAILED => Self::Failed,
            _ => Self::Other(code),
        }
    }
}
