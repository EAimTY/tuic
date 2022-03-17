use super::UdpSessionMap;
use anyhow::{anyhow, Result};
use bytes::{Bytes, BytesMut};
use quinn::{Connection as QuinnConnection, RecvStream, SendStream};
use std::{net::ToSocketAddrs, sync::Arc};
use tokio::{io, net::TcpStream};
use tuic_protocol::{Address, Command, Response};

pub async fn connect(mut send: SendStream, mut recv: RecvStream, addr: Address) {
    let res: Result<()> = try {
        let mut stream = None;
        let addrs = addr.to_socket_addrs()?;

        for addr in addrs {
            if let Ok(tcp_stream) = TcpStream::connect(addr).await {
                stream = Some(tcp_stream);
            }
        }

        let mut stream = if let Some(stream) = stream {
            stream
        } else {
            Err(anyhow!("Failed to connect to remote"))?
        };

        let resp = Response::new(true);
        resp.write_to(&mut send).await?;

        let (mut target_recv, mut target_send) = stream.split();
        let target_to_tunnel = io::copy(&mut target_recv, &mut send);
        let tunnel_to_target = io::copy(&mut recv, &mut target_send);
        let _ = tokio::try_join!(target_to_tunnel, tunnel_to_target);
    };

    match res {
        Ok(()) => {}
        Err(err) => eprintln!("{err}"),
    }
}

pub async fn bind(_send: SendStream, _recv: RecvStream, _addr: Address) {
    todo!()
}

pub async fn packet_from_uni_stream(
    mut stream: RecvStream,
    udp_sessions: Arc<UdpSessionMap>,
    assoc_id: u32,
    len: u16,
    addr: Address,
) {
    let mut buf = vec![0; len as usize];

    match stream.read_exact(&mut buf).await {
        Ok(()) => udp_sessions.send(assoc_id, Bytes::from(buf), addr).await,
        Err(err) => eprintln!("{err}"),
    }
}

pub async fn packet_from_datagram(
    pkt: Bytes,
    udp_sessions: Arc<UdpSessionMap>,
    assoc_id: u32,
    addr: Address,
) {
    udp_sessions.send(assoc_id, pkt, addr).await;
}

pub async fn packet_to_uni_stream(conn: QuinnConnection, assoc_id: u32, pkt: Bytes, addr: Address) {
    let res: Result<()> = try {
        let mut stream = conn.open_uni().await?;

        let cmd = Command::new_packet(assoc_id, pkt.len() as u16, addr);
        cmd.write_to(&mut stream).await?;

        stream.write_all(&pkt).await?;
    };

    match res {
        Ok(()) => {}
        Err(err) => eprintln!("{err}"),
    }
}

pub async fn packet_to_datagram(conn: QuinnConnection, assoc_id: u32, pkt: Bytes, addr: Address) {
    let res: Result<()> = try {
        let cmd = Command::new_packet(assoc_id, pkt.len() as u16, addr);

        let mut buf = BytesMut::with_capacity(cmd.serialized_len());
        cmd.write_to_buf(&mut buf);
        buf.extend_from_slice(&pkt);
        let pkt = buf.freeze();

        conn.send_datagram(pkt)?;
    };

    match res {
        Ok(()) => {}
        Err(err) => eprintln!("{err}"),
    }
}

pub async fn dissociate(udp_sessions: Arc<UdpSessionMap>, assoc_id: u32) {
    udp_sessions.dissociate(assoc_id);
}
