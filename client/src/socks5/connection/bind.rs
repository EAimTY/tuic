use super::Connection;
use crate::socks5::protocol::Address;
use anyhow::Result;

impl Connection {
    pub async fn handle_bind(self, _addr: Address) -> Result<()> {
        Ok(())
    }
}
