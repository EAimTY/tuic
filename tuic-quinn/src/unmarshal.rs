use async_trait::async_trait;
use futures_util::AsyncRead;

#[async_trait]
trait Unmarshal {
    fn unmarshal(&self, s: &mut impl AsyncRead) -> Result<(), ()>;
}
