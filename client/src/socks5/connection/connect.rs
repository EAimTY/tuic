use super::Connection;
use crate::{
    relay::{Address as RelayAddress, Request as RelayRequest},
    socks5::protocol::{Address, Reply, Response},
};
use anyhow::Result;
use std::net::SocketAddr;
use tokio::io;

impl Connection {
    pub async fn handle_connect(mut self, addr: Address) -> Result<()> {
        let addr = RelayAddress::from(addr);
        let (relay_req, relay_resp_rx) = RelayRequest::new_connect(addr);

        let _ = self.req_tx.send(relay_req).await;
        let relay_resp = relay_resp_rx.await?;

        if let Some((mut remote_send, mut remote_recv)) = relay_resp {
            let resp = Response::new(
                Reply::Succeeded,
                Address::SocketAddress(SocketAddr::from(([0, 0, 0, 0], 0))),
            );

            resp.write_to(&mut self.stream).await?;

            let (mut local_recv, mut local_send) = self.stream.split();
            let remote_to_local = io::copy(&mut remote_recv, &mut local_send);
            let local_to_remote = io::copy(&mut local_recv, &mut remote_send);

            let _ = tokio::try_join!(remote_to_local, local_to_remote);
        } else {
            let resp = Response::new(
                Reply::NetworkUnreachable,
                Address::SocketAddress(SocketAddr::from(([0, 0, 0, 0], 0))),
            );

            resp.write_to(&mut self.stream).await?;
        }

        Ok(())
    }
}
