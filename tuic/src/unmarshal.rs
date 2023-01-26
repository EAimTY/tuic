use crate::protocol::Header;
use async_trait::async_trait;
use futures_io::AsyncRead;
use thiserror::Error;

#[async_trait]
pub trait Unmarshal
where
    Self: Sized,
{
    async fn unmarshal(s: &mut impl AsyncRead) -> Result<Self, UnmarshalError>;
}

#[async_trait]
impl Unmarshal for Header {
    async fn unmarshal(s: &mut impl AsyncRead) -> Result<Self, UnmarshalError> {
        todo!()
    }
}

#[derive(Debug, Error)]
pub enum UnmarshalError {}
