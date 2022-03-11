use super::Connection;
use crate::client::{Address as RelayAddress, Request as RelayRequest};
use crate::socks5::protocol::{Address, Reply, Response, UdpHeader};
use anyhow::{bail, Result};
use bytes::Bytes;
use std::net::SocketAddr;
use tokio::{net::UdpSocket, sync::mpsc::Sender as MpscSender};

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

        let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?;
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

        socket.connect(src_addr.unwrap()).await?;

        loop {
            let mut buf = vec![0; 1536];
            let len = socket.recv(&mut buf).await?;
            buf.truncate(len);

            match send_packet(buf, &pkt_send_tx).await {
                Ok(()) => (),
                Err(err) => eprintln!("{err}"),
            }
        }

        Ok(())
    }
}

async fn send_packet(buf: Vec<u8>, pkt_send_tx: &MpscSender<(Bytes, RelayAddress)>) -> Result<()> {
    let udp_header = UdpHeader::read_from(&mut buf.as_slice()).await?;

    if udp_header.frag != 0 {
        bail!("Fragmented UDP packet is not supported");
    }

    let bytes = Bytes::from(buf).slice(udp_header.serialized_len()..);

    let dst_addr = match udp_header.address {
        Address::SocketAddress(addr) => RelayAddress::SocketAddress(addr),
        Address::HostnameAddress(hostname, port) => RelayAddress::HostnameAddress(hostname, port),
    };

    let _ = pkt_send_tx.send((bytes, dst_addr)).await;

    Ok(())
}
