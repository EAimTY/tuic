use crate::protocol::Header;
use futures_io::AsyncRead;
use thiserror::Error;

impl Header {
    pub async fn async_unmarshal(s: &mut impl AsyncRead) -> Result<Self, UnmarshalError> {
        todo!()
    }
}

#[derive(Debug, Error)]
pub enum UnmarshalError {}
