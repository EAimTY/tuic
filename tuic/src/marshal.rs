use crate::protocol::Header;
use futures_io::AsyncWrite;
use std::io::Error as IoError;

impl Header {
    pub async fn async_marshal(&self, s: &mut impl AsyncWrite) -> Result<(), IoError> {
        todo!()
    }
}
