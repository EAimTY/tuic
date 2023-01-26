use async_trait::async_trait;
use futures_util::AsyncWrite;
use std::io::Error as IoError;
use tuic::protocol::Header;

#[async_trait]
pub(super) trait Marshal {
    async fn marshal(&self, s: &mut impl AsyncWrite) -> Result<(), IoError>;
}

#[async_trait]
impl Marshal for Header {
    async fn marshal(&self, s: &mut impl AsyncWrite) -> Result<(), IoError> {
        todo!()
    }
}
