use async_trait::async_trait;
use futures_util::AsyncRead;
use thiserror::Error;
use tuic::protocol::Header;

#[async_trait]
pub(super) trait Unmarshal
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
