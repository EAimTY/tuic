use crate::relay::{self, Address as RelayAddress, Request as RelayRequest};
use bytes::Bytes;
use socks5_proto::{Address, Reply, UdpHeader};
use socks5_server::{
    connection::associate::{AssociatedUdpSocket, NeedReply},
    Associate,
};
use std::{
    io::Result,
    net::SocketAddr,
    sync::{atomic::Ordering, Arc},
};
use tokio::{
    net::UdpSocket,
    sync::mpsc::{Receiver, Sender},
};
use tuic_protocol::Command as TuicCommand;

pub async fn handle(
    conn: Associate<NeedReply>,
    req_tx: Sender<RelayRequest>,
    target_addr: Address,
) -> Result<()> {
    async fn bind_udp_socket(conn: &Associate<NeedReply>) -> Result<UdpSocket> {
        UdpSocket::bind(SocketAddr::from((conn.local_addr()?.ip(), 0))).await
    }

    log::info!(
        "[socks5] [{}] [associate] [{target_addr}]",
        conn.peer_addr()?
    );

    match bind_udp_socket(&conn)
        .await
        .and_then(|socket| socket.local_addr().map(|addr| (socket, addr)))
    {
        Ok((socket, socket_addr)) => {
            let (relay_req, pkt_send_tx, pkt_recv_rx) = RelayRequest::new_associate();
            let _ = req_tx.send(relay_req).await;

            let mut conn = conn
                .reply(Reply::Succeeded, Address::SocketAddress(socket_addr))
                .await?;

            let buf_size = relay::MAX_UDP_RELAY_PACKET_SIZE.load(Ordering::Acquire)
                - (TuicCommand::max_serialized_len() - UdpHeader::max_serialized_len());
            let socket = Arc::new(AssociatedUdpSocket::from((socket, buf_size)));
            let ctrl_addr = conn.peer_addr()?;

            let res = tokio::select! {
                _ = conn.wait_until_closed() => Ok(()),
                res = socks5_to_relay(socket.clone(),ctrl_addr, pkt_send_tx) => res,
                res = relay_to_socks5(socket,ctrl_addr, pkt_recv_rx) => res,
            };

            let _ = conn.shutdown().await;

            log::info!("[socks5] [{ctrl_addr}] [dissociate] [{target_addr}]");

            res
        }
        Err(err) => {
            let mut conn = conn
                .reply(Reply::GeneralFailure, Address::unspecified())
                .await?;

            let _ = conn.shutdown().await;
            Err(err)
        }
    }
}

async fn socks5_to_relay(
    socket: Arc<AssociatedUdpSocket>,
    ctrl_addr: SocketAddr,
    pkt_send_tx: Sender<(Bytes, RelayAddress)>,
) -> Result<()> {
    loop {
        let buf_size = relay::MAX_UDP_RELAY_PACKET_SIZE.load(Ordering::Acquire)
            - (TuicCommand::max_serialized_len() - UdpHeader::max_serialized_len());
        socket.set_max_packet_size(buf_size);

        let (pkt, frag, dst_addr, src_addr) = socket.recv_from().await?;

        if frag == 0 {
            log::debug!("[socks5] [{ctrl_addr}] [associate] [packet-to] {dst_addr}");

            let dst_addr = match dst_addr {
                Address::DomainAddress(domain, port) => RelayAddress::DomainAddress(domain, port),
                Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
            };

            let _ = pkt_send_tx.send((pkt, dst_addr)).await;
            socket.connect(src_addr).await?;
            break;
        } else {
            log::warn!("[socks5] [{ctrl_addr}] [associate] [packet-to] socks5 UDP packet fragment is not supported");
        }
    }

    loop {
        let buf_size = relay::MAX_UDP_RELAY_PACKET_SIZE.load(Ordering::Acquire)
            - (TuicCommand::max_serialized_len() - UdpHeader::max_serialized_len());
        socket.set_max_packet_size(buf_size);

        let (pkt, frag, dst_addr) = socket.recv().await?;

        if frag == 0 {
            log::debug!("[socks5] [{ctrl_addr}] [associate] [packet-to] {dst_addr}");

            let dst_addr = match dst_addr {
                Address::DomainAddress(domain, port) => RelayAddress::DomainAddress(domain, port),
                Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
            };

            let _ = pkt_send_tx.send((pkt, dst_addr)).await;
        } else {
            log::warn!("[socks5] [{ctrl_addr}] [associate] [packet-to] socks5 UDP packet fragment is not supported");
        }
    }
}

async fn relay_to_socks5(
    socket: Arc<AssociatedUdpSocket>,
    ctrl_addr: SocketAddr,
    mut pkt_recv_rx: Receiver<(Bytes, RelayAddress)>,
) -> Result<()> {
    while let Some((pkt, src_addr)) = pkt_recv_rx.recv().await {
        log::debug!("[socks5] [{ctrl_addr}] [associate] [packet-from] {src_addr}");

        let src_addr = match src_addr {
            RelayAddress::DomainAddress(domain, port) => Address::DomainAddress(domain, port),
            RelayAddress::SocketAddress(addr) => Address::SocketAddress(addr),
        };

        socket.send(pkt, 0, src_addr).await?;
    }

    Ok(())
}
