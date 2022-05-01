use crate::relay::{Address as RelayAddress, Request as RelayRequest};
use bytes::Bytes;
use socks5_proto::{Address, Reply};
use socks5_server::{
    connection::associate::{AssociateUdpSocket, NeedReply},
    Associate,
};
use std::{io::Error as IoError, net::SocketAddr, sync::Arc};
use tokio::{
    io::AsyncWriteExt,
    net::UdpSocket,
    sync::mpsc::{Receiver, Sender},
};

pub async fn handle(
    conn: Associate<NeedReply>,
    req_tx: Sender<RelayRequest>,
    _target_addr: Address,
) -> Result<(), IoError> {
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

            let socket = Arc::new(AssociateUdpSocket::from(socket));

            let res = tokio::select! {
                _ = conn.wait_for_close() => Ok(()),
                res = socks5_to_relay(socket.clone(), pkt_send_tx) => res,
                res = relay_to_socks5(socket, pkt_recv_rx) => res,
            };

            let _ = conn.shutdown().await;
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

async fn bind_udp_socket(conn: &Associate<NeedReply>) -> Result<UdpSocket, IoError> {
    UdpSocket::bind(SocketAddr::from((conn.local_addr()?.ip(), 0))).await
}

async fn socks5_to_relay(
    socket: Arc<AssociateUdpSocket>,
    pkt_send_tx: Sender<(Bytes, RelayAddress)>,
) -> Result<(), IoError> {
    loop {
        let (pkt, frag, dst_addr, src_addr) = socket.recv_from().await?;

        if frag == 0 {
            let dst_addr = match dst_addr {
                Address::DomainAddress(domain, port) => RelayAddress::DomainAddress(domain, port),
                Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
            };

            let _ = pkt_send_tx.send((pkt, dst_addr)).await;
            socket.connect(src_addr).await?;
            break;
        }
    }

    loop {
        let (pkt, frag, dst_addr) = socket.recv().await?;

        if frag == 0 {
            let dst_addr = match dst_addr {
                Address::DomainAddress(domain, port) => RelayAddress::DomainAddress(domain, port),
                Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
            };

            let _ = pkt_send_tx.send((pkt, dst_addr)).await;
        }
    }
}

async fn relay_to_socks5(
    socket: Arc<AssociateUdpSocket>,
    mut pkt_recv_rx: Receiver<(Bytes, RelayAddress)>,
) -> Result<(), IoError> {
    while let Some((pkt, src_addr)) = pkt_recv_rx.recv().await {
        let src_addr = match src_addr {
            RelayAddress::DomainAddress(domain, port) => Address::DomainAddress(domain, port),
            RelayAddress::SocketAddress(addr) => Address::SocketAddress(addr),
        };

        socket.send(pkt, 0, src_addr).await?;
    }

    Ok(())
}
