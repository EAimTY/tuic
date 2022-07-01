use super::udp::UdpSessionMap;
use bytes::{Bytes, BytesMut};
use quinn::{
    Connection as QuinnConnection, ConnectionError, ReadExactError, RecvStream, SendDatagramError,
    SendStream, WriteError,
};
use std::{
    io::{Error as IoError, IoSlice},
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use thiserror::Error;
use tokio::{
    io::{self, AsyncRead, AsyncWrite, ReadBuf},
    net::{self, TcpStream},
};
use tuic_protocol::{Address, Command};

pub async fn connect(
    mut send: SendStream,
    recv: RecvStream,
    addr: Address,
) -> Result<(), TaskError> {
    let mut target = None;

    let addrs = match addr {
        Address::SocketAddress(addr) => Ok(vec![addr]),
        Address::DomainAddress(domain, port) => net::lookup_host((domain.as_str(), port))
            .await
            .map(|res| res.collect()),
    }?;

    for addr in addrs {
        if let Ok(target_stream) = TcpStream::connect(addr).await {
            target = Some(target_stream);
            break;
        }
    }

    if let Some(mut target) = target {
        let resp = Command::new_response(true);
        resp.write_to(&mut send).await?;
        let mut tunnel = BiStream(send, recv);
        io::copy_bidirectional(&mut target, &mut tunnel).await?;
    } else {
        let resp = Command::new_response(false);
        resp.write_to(&mut send).await?;
        send.finish().await?;
    };

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
    stream.finish().await?;

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

pub async fn dissociate(
    udp_sessions: Arc<UdpSessionMap>,
    assoc_id: u32,
    src_addr: SocketAddr,
) -> Result<(), TaskError> {
    udp_sessions.dissociate(assoc_id, src_addr);
    Ok(())
}

struct BiStream(SendStream, RecvStream);

impl AsyncRead for BiStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        Pin::new(&mut self.1).poll_read(cx, buf)
    }
}

impl AsyncWrite for BiStream {
    #[inline]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    #[inline]
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize, IoError>> {
        Pin::new(&mut self.0).poll_write_vectored(cx, bufs)
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        self.0.is_write_vectored()
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    #[inline]
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
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
