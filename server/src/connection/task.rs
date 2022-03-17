use super::udp::UdpSessionMap;
use bytes::{Bytes, BytesMut};
use quinn::{
    Connection as QuinnConnection, ConnectionError, ReadExactError, RecvStream, SendDatagramError,
    SendStream, WriteError,
};
use std::{
    io::Error as IoError,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};
use thiserror::Error;
use tokio::{io, net::TcpStream};
use tuic_protocol::{Address, Command, Response};

pub async fn connect(
    mut send: SendStream,
    mut recv: RecvStream,
    addr: Address,
) -> Result<(), TaskError> {
    let mut stream = None;
    let addrs = addr.to_socket_addrs()?;

    for addr in addrs {
        if let Ok(tcp_stream) = TcpStream::connect(addr).await {
            stream = Some(tcp_stream);
            break;
        }
    }

    if let Some(mut stream) = stream {
        let resp = Response::new(true);
        resp.write_to(&mut send).await?;

        let (mut target_recv, mut target_send) = stream.split();
        let target_to_tunnel = io::copy(&mut target_recv, &mut send);
        let tunnel_to_target = io::copy(&mut recv, &mut target_send);
        let _ = tokio::try_join!(target_to_tunnel, tunnel_to_target);
    } else {
        let resp = Response::new(false);
        resp.write_to(&mut send).await?;
    };

    Ok(())
}

pub async fn bind(_send: SendStream, _recv: RecvStream, _addr: Address) -> Result<(), TaskError> {
    Ok(())
}

pub async fn packet_from_uni_stream(
    mut stream: RecvStream,
    udp_sessions: Arc<UdpSessionMap>,
    assoc_id: u32,
    len: u16,
    addr: Address,
    src_addr: SocketAddr,
) -> Result<(), TaskError> {
    let mut buf = vec![0; len as usize];
    stream.read_exact(&mut buf).await?;

    let pkt = Bytes::from(buf);
    udp_sessions.send(assoc_id, pkt, addr, src_addr).await?;

    Ok(())
}

pub async fn packet_from_datagram(
    pkt: Bytes,
    udp_sessions: Arc<UdpSessionMap>,
    assoc_id: u32,
    addr: Address,
    src_addr: SocketAddr,
) -> Result<(), TaskError> {
    udp_sessions.send(assoc_id, pkt, addr, src_addr).await?;
    Ok(())
}

pub async fn packet_to_uni_stream(
    conn: QuinnConnection,
    assoc_id: u32,
    pkt: Bytes,
    addr: Address,
) -> Result<(), TaskError> {
    let mut stream = conn.open_uni().await?;

    let cmd = Command::new_packet(assoc_id, pkt.len() as u16, addr);
    cmd.write_to(&mut stream).await?;

    stream.write_all(&pkt).await?;

    Ok(())
}

pub async fn packet_to_datagram(
    conn: QuinnConnection,
    assoc_id: u32,
    pkt: Bytes,
    addr: Address,
) -> Result<(), TaskError> {
    let cmd = Command::new_packet(assoc_id, pkt.len() as u16, addr);

    let mut buf = BytesMut::with_capacity(cmd.serialized_len());
    cmd.write_to_buf(&mut buf);
    buf.extend_from_slice(&pkt);

    let pkt = buf.freeze();
    conn.send_datagram(pkt)?;

    Ok(())
}

pub async fn dissociate(udp_sessions: Arc<UdpSessionMap>, assoc_id: u32) -> Result<(), TaskError> {
    udp_sessions.dissociate(assoc_id);
    Ok(())
}

#[derive(Error, Debug)]
pub enum TaskError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    ReadStream(#[from] ReadExactError),
    #[error(transparent)]
    WriteStream(#[from] WriteError),
    #[error(transparent)]
    SendDatagram(#[from] SendDatagramError),
}
