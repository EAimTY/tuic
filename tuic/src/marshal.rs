use crate::protocol::Header;
use async_trait::async_trait;
use futures_io::AsyncWrite;
use std::io::Error as IoError;

#[async_trait]
pub trait Marshal {
    async fn marshal(&self, s: &mut impl AsyncWrite) -> Result<(), IoError>;
}

#[async_trait]
impl Marshal for Header {
    async fn marshal(&self, s: &mut impl AsyncWrite) -> Result<(), IoError> {
        todo!()
    }
}
