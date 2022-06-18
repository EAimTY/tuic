use crate::relay::{Address as RelayAddress, Request as RelayRequest};
use socks5_proto::{Address, Reply};
use socks5_server::{connection::connect::NeedReply, Connect};
use std::io::Result;
use tokio::{io, sync::mpsc::Sender};

pub async fn handle(
    conn: Connect<NeedReply>,
    req_tx: Sender<RelayRequest>,
    target_addr: Address,
) -> Result<()> {
    log::info!("[socks5] [{}] [connect] [{target_addr}]", conn.peer_addr()?);

    let target_addr = match target_addr {
        Address::DomainAddress(domain, port) => RelayAddress::DomainAddress(domain, port),
        Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
    };

    let (relay_req, relay_resp_rx) = RelayRequest::new_connect(target_addr);
    let _ = req_tx.send(relay_req).await;

    if let Ok(mut relay) = relay_resp_rx.await {
        let mut conn = conn.reply(Reply::Succeeded, Address::unspecified()).await?;
        io::copy_bidirectional(&mut conn, &mut relay).await?;
    } else {
        let mut conn = conn
            .reply(Reply::NetworkUnreachable, Address::unspecified())
            .await?;

        let _ = conn.shutdown().await;
    }

    Ok(())
}
