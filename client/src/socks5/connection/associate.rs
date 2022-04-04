use super::Connection;
use crate::{
    relay::{Address as RelayAddress, Request as RelayRequest},
    socks5::{
        protocol::{Address, Reply, Response, UdpHeader},
        Socks5Error,
    },
};
use bytes::{Bytes, BytesMut};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::AsyncReadExt,
    net::{TcpStream, UdpSocket},
    sync::mpsc::{Receiver, Sender},
};

impl Connection {
    pub async fn handle_associate(
        mut self,
        ctrl_addr: SocketAddr,
        max_udp_pkt_size: usize,
    ) -> Result<(), Socks5Error> {
        match create_udp_socket().await {
            Ok((socket, socket_addr)) => {
                let socket = Arc::new(socket);

                let resp = Response::new(Reply::Succeeded, Address::SocketAddress(socket_addr));
                resp.write_to(&mut self.stream).await?;

                let (relay_req, pkt_send_tx, pkt_receive_rx) = RelayRequest::new_associate();
                let _ = self.req_tx.send(relay_req).await;

                let res = tokio::select! {
                    res = listen_packet_to_relay(socket.clone(), ctrl_addr, max_udp_pkt_size, pkt_send_tx) => res,
                    res = listen_packet_from_relay(socket, ctrl_addr, pkt_receive_rx) => res,
                    () = listen_control_stream(self.stream) => Ok(()),
                };

                match res {
                    Ok(()) => {}
                    Err(err) => log::warn!("[socks5] [{ctrl_addr}] [associate] {err}"),
                }

                Ok(())
            }
            Err(err) => {
                let resp = Response::new(
                    Reply::GeneralFailure,
                    Address::SocketAddress(SocketAddr::from(([0, 0, 0, 0], 0))),
                );

                resp.write_to(&mut self.stream).await?;

                Err(err)
            }
        }
    }
}

async fn create_udp_socket() -> Result<(UdpSocket, SocketAddr), Socks5Error> {
    let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?;
    let addr = socket.local_addr()?;

    Ok((socket, addr))
}

async fn listen_packet_to_relay(
    socket: Arc<UdpSocket>,
    ctrl_addr: SocketAddr,
    max_udp_pkt_size: usize,
    pkt_send_tx: Sender<(Bytes, RelayAddress)>,
) -> Result<(), Socks5Error> {
    loop {
        let mut buf = vec![0; max_udp_pkt_size];
        let (len, addr) = socket.recv_from(&mut buf).await?;
        buf.truncate(len);
        let pkt = Bytes::from(buf);

        match process_packet_to_relay(pkt).await {
            Ok((pkt, dst_addr)) => {
                log::debug!("[socks5] [{ctrl_addr}] [associate] [packet-to] {dst_addr}");
                socket.connect(addr).await?;
                let _ = pkt_send_tx.send((pkt, dst_addr)).await;
                break;
            }
            Err(err) => log::debug!("[socks5] [{ctrl_addr}] [associate] [packet-to] {err}"),
        }
    }

    loop {
        let mut buf = vec![0; max_udp_pkt_size];
        let len = socket.recv(&mut buf).await?;
        buf.truncate(len);
        let pkt = Bytes::from(buf);

        match process_packet_to_relay(pkt).await {
            Ok((pkt, dst_addr)) => {
                log::debug!("[socks5] [{ctrl_addr}] [associate] [packet-to] {dst_addr}");
                let _ = pkt_send_tx.send((pkt, dst_addr)).await;
            }
            Err(err) => log::debug!("[socks5] [{ctrl_addr}] [associate] [packet-to] {err}"),
        }
    }
}

async fn listen_packet_from_relay(
    socket: Arc<UdpSocket>,
    ctrl_addr: SocketAddr,
    mut pkt_receive_rx: Receiver<(Bytes, RelayAddress)>,
) -> Result<(), Socks5Error> {
    while let Some((pkt, addr)) = pkt_receive_rx.recv().await {
        log::debug!("[socks5] [{ctrl_addr}] [associate] [packet-from] {addr}");
        let pkt = process_packet_from_relay(pkt, addr);
        socket.send(&pkt).await?;
    }

    Err(Socks5Error::RelayConnectivity)
}

async fn listen_control_stream(mut stream: TcpStream) {
    while let true = stream.read_u8().await.is_ok() {}
}

async fn process_packet_to_relay(pkt: Bytes) -> Result<(Bytes, RelayAddress), Socks5Error> {
    let header = UdpHeader::read_from(&mut pkt.as_ref()).await?;

    if header.frag != 0 {
        return Err(Socks5Error::FragmentedUdpPacket);
    }

    let pkt = pkt.slice(header.serialized_len()..);
    let addr = RelayAddress::from(header.address);

    Ok((pkt, addr))
}

fn process_packet_from_relay(pkt: Bytes, addr: RelayAddress) -> Bytes {
    let addr = Address::from(addr);
    let header = UdpHeader::new(0, addr);

    let mut buf = BytesMut::with_capacity(header.serialized_len());
    header.write_to_buf(&mut buf);
    buf.extend_from_slice(&pkt);
    buf.freeze()
}
