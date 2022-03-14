use super::Connection;
use quinn::Datagrams;

impl Connection {
    pub async fn listen_datagrams(&self, datagrams: Datagrams) {}
}
