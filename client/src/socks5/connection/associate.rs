use super::Connection;
use crate::{
    relay::{Address as RelayAddress, Request as RelayRequest},
    socks5::protocol::{Address, Reply, Response, UdpHeader},
};
use anyhow::{bail, Result};
use bytes::Bytes;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::AsyncReadExt,
    net::{TcpStream, UdpSocket},
    sync::mpsc::{Receiver as MpscReceiver, Sender as MpscSender},
};

impl Connection {
    pub async fn handle_associate(mut self, addr: Address) -> Result<()> {
        let mut src_addr = match addr {
            Address::SocketAddress(addr) => {
                if addr.ip().is_unspecified() && addr.port() == 0 {
                    None
                } else {
                    Some(addr)
                }
            }
            Address::HostnameAddress(hostname, port) => {
                if hostname.is_empty() && port == 0 {
                    None
                } else {
                    bail!("Connot associate FQDN address")
                }
            }
        };

        let socket = Arc::new(UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?);
        let socket_addr = socket.local_addr()?;

        let resp = Response::new(Reply::Succeeded, Address::SocketAddress(socket_addr));
        resp.write_to(&mut self.stream).await?;

        let (relay_req, pkt_send_tx, pkt_receive_rx) = RelayRequest::new_associate();
        let _ = self.req_tx.send(relay_req).await;

        if src_addr.is_none() {
            let mut buf = vec![0; 1536];
            let (len, addr) = socket.recv_from(&mut buf).await?;
            buf.truncate(len);

            src_addr = Some(addr);

            match send_packet(buf, &pkt_send_tx).await {
                Ok(()) => (),
                Err(err) => eprintln!("{err}"),
            }
        }

        let src_addr = unsafe { src_addr.unwrap_unchecked() };
        socket.connect(src_addr).await?;

        tokio::try_join!(
            listen_send(socket.clone(), pkt_send_tx),
            listen_receive(socket, pkt_receive_rx),
            listen_termination(self.stream)
        )?;

        Ok(())
    }
}

async fn listen_send(
    socket: Arc<UdpSocket>,
    pkt_send_tx: MpscSender<(Bytes, RelayAddress)>,
) -> Result<()> {
    loop {
        let mut buf = vec![0; 1536];
        let len = socket.recv(&mut buf).await?;
        buf.truncate(len);

        match send_packet(buf, &pkt_send_tx).await {
            Ok(()) => (),
            Err(err) => eprintln!("{err}"),
        }
    }
}

async fn listen_receive(
    socket: Arc<UdpSocket>,
    mut pkt_receive_rx: MpscReceiver<(Bytes, RelayAddress)>,
) -> Result<()> {
    while let Some((packet, addr)) = pkt_receive_rx.recv().await {
        let addr = Address::from(addr);

        let udp_header = UdpHeader::new(0, addr);

        let mut buf = vec![0; udp_header.serialized_len() + packet.len()];
        udp_header.write_to_buf(&mut buf);
        buf[udp_header.serialized_len()..].copy_from_slice(&packet);

        socket.send(&buf).await?;
    }

    bail!("UDP packet channel closed");
}

async fn listen_termination(mut stream: TcpStream) -> Result<()> {
    let mut buf = [0; 8];

    loop {
        stream.read_exact(&mut buf).await?;
    }
}

async fn send_packet(buf: Vec<u8>, pkt_send_tx: &MpscSender<(Bytes, RelayAddress)>) -> Result<()> {
    let udp_header = UdpHeader::read_from(&mut buf.as_slice()).await?;

    if udp_header.frag != 0 {
        bail!("Fragmented UDP packet is not supported");
    }

    let packet = Bytes::from(buf).slice(udp_header.serialized_len()..);
    let dst_addr = RelayAddress::from(udp_header.address);

    let _ = pkt_send_tx.send((packet, dst_addr)).await;

    Ok(())
}
