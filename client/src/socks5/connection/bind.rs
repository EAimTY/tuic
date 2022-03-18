use super::Connection;
use crate::socks5::{protocol::Address, Socks5Error};

impl Connection {
    pub async fn handle_bind(self, _addr: Address) -> Result<(), Socks5Error> {
        Ok(())
    }
}
