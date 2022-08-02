use super::{
    stream::{RecvStream, SendStream, StreamReg},
    ConnectError, Stream,
};
use crate::{
    common::{self, PacketBuffer},
    protocol::{Address, Command, Error as TuicError},
    Packet, PacketBufferError, UdpRelayMode,
};
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use quinn::{
    Connecting as QuinnConnecting, Connection as QuinnConnection,
    ConnectionError as QuinnConnectionError, Datagrams, IncomingUniStreams,
    NewConnection as QuinnNewConnection, RecvStream as QuinnRecvStream,
};
use std::{
    io::{Error as IoError, ErrorKind, Result as IoResult},
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time,
};

#[derive(Debug)]
pub struct Connecting {
    conn: QuinnConnecting,
    token: [u8; 32],
    udp_relay_mode: UdpRelayMode,
}

impl Connecting {
    pub(super) fn new(
        conn: QuinnConnecting,
        token: [u8; 32],
        udp_relay_mode: UdpRelayMode,
    ) -> Self {
        Self {
            conn,
            token,
            udp_relay_mode,
        }
    }

    pub async fn establish(self) -> Result<(Connection, IncomingPackets), ConnectError> {
        let QuinnNewConnection {
            connection,
            datagrams,
            uni_streams,
            ..
        } = match self.conn.await {
            Ok(conn) => conn,
            Err(err) => return Err(ConnectError::from_quinn_connection_error(err)),
        };

        Ok(Connection::new(
            connection,
            uni_streams,
            datagrams,
            self.token,
            self.udp_relay_mode,
        ))
    }
}

#[derive(Debug)]
pub struct Connection {
    conn: QuinnConnection,
    token: [u8; 32],
    udp_relay_mode: UdpRelayMode,
    stream_reg: Arc<StreamReg>,
    next_pkt_id: Arc<AtomicU16>,
}

impl Connection {
    pub(super) fn new(
        conn: QuinnConnection,
        uni_streams: IncomingUniStreams,
        datagrams: Datagrams,
        token: [u8; 32],
        udp_relay_mode: UdpRelayMode,
    ) -> (Self, IncomingPackets) {
        let stream_reg = Arc::new(Arc::new(()));

        let conn = Self {
            conn,
            token,
            udp_relay_mode,
            stream_reg: stream_reg.clone(),
            next_pkt_id: Arc::new(AtomicU16::new(0)),
        };

        let incoming = IncomingPackets {
            uni_streams,
            datagrams,
            udp_relay_mode,
            stream_reg,
            pkt_buf: PacketBuffer::new(),
            last_gc_time: Instant::now(),
        };

        (conn, incoming)
    }

    pub async fn authenticate(&self) -> IoResult<()> {
        let mut send = self.get_send_stream().await?;
        let cmd = Command::Authenticate(self.token);
        cmd.write_to(&mut send).await?;
        send.finish().await?;
        Ok(())
    }

    pub async fn heartbeat(&self) -> IoResult<()> {
        let mut send = self.get_send_stream().await?;
        let cmd = Command::Heartbeat;
        cmd.write_to(&mut send).await?;
        send.finish().await?;
        Ok(())
    }

    pub async fn connect(&self, addr: Address) -> IoResult<Option<Stream>> {
        let mut stream = self.get_bi_stream().await?;

        let cmd = Command::Connect { addr };
        cmd.write_to(&mut stream).await?;

        let resp = match Command::read_from(&mut stream).await {
            Ok(Command::Response(resp)) => Ok(resp),
            Ok(cmd) => Err(TuicError::InvalidCommand(cmd.as_type_code())),
            Err(err) => Err(err),
        };

        let res = match resp {
            Ok(true) => return Ok(Some(stream)),
            Ok(false) => Ok(None),
            Err(err) => Err(IoError::from(err)),
        };

        stream.finish().await?;
        res
    }

    pub async fn packet(&self, assoc_id: u32, addr: Address, pkt: Bytes) -> IoResult<()> {
        match self.udp_relay_mode {
            UdpRelayMode::Native => self.send_packet_to_datagram(assoc_id, addr, pkt),
            UdpRelayMode::Quic => self.send_packet_to_uni_stream(assoc_id, addr, pkt).await,
        }
    }

    pub async fn dissociate(&self, assoc_id: u32) -> IoResult<()> {
        let mut send = self.get_send_stream().await?;
        let cmd = Command::Dissociate { assoc_id };
        cmd.write_to(&mut send).await?;
        send.finish().await?;
        Ok(())
    }

    async fn get_send_stream(&self) -> IoResult<SendStream> {
        let send = self.conn.open_uni().await?;
        Ok(SendStream::new(send, self.stream_reg.as_ref().clone()))
    }

    async fn get_bi_stream(&self) -> IoResult<Stream> {
        let (send, recv) = self.conn.open_bi().await?;
        let send = SendStream::new(send, self.stream_reg.as_ref().clone());
        let recv = RecvStream::new(recv, self.stream_reg.as_ref().clone());
        Ok(Stream::new(send, recv))
    }

    fn send_packet_to_datagram(&self, assoc_id: u32, addr: Address, pkt: Bytes) -> IoResult<()> {
        let max_datagram_size = if let Some(size) = self.conn.max_datagram_size() {
            size
        } else {
            return Err(IoError::new(ErrorKind::Other, "datagram not supported"));
        };

        let pkt_id = self.next_pkt_id.fetch_add(1, Ordering::SeqCst);
        let mut pkts = common::split_packet(pkt, &addr, max_datagram_size);
        let frag_total = pkts.len() as u8;

        let first_pkt = pkts.next().unwrap();
        let first_pkt_header = Command::Packet {
            assoc_id,
            pkt_id,
            frag_total,
            frag_id: 0,
            len: first_pkt.len() as u16,
            addr: Some(addr),
        };

        let mut buf = BytesMut::with_capacity(first_pkt_header.serialized_len() + first_pkt.len());
        first_pkt_header.write_to_buf(&mut buf);
        buf.extend_from_slice(&first_pkt);
        let buf = buf.freeze();

        self.conn.send_datagram(buf).unwrap(); // TODO: error handling

        for (id, pkt) in pkts.enumerate() {
            let pkt_header = Command::Packet {
                assoc_id,
                pkt_id,
                frag_total,
                frag_id: id as u8 + 1,
                len: pkt.len() as u16,
                addr: None,
            };

            let mut buf = BytesMut::with_capacity(pkt_header.serialized_len() + pkt.len());
            pkt_header.write_to_buf(&mut buf);
            buf.extend_from_slice(&pkt);
            let buf = buf.freeze();

            self.conn.send_datagram(buf).unwrap(); // TODO: error handling
        }

        Ok(())
    }

    async fn send_packet_to_uni_stream(
        &self,
        assoc_id: u32,
        addr: Address,
        pkt: Bytes,
    ) -> IoResult<()> {
        let mut send = self.get_send_stream().await?;

        let cmd = Command::Packet {
            assoc_id,
            pkt_id: self.next_pkt_id.fetch_add(1, Ordering::SeqCst),
            frag_total: 1,
            frag_id: 0,
            len: pkt.len() as u16,
            addr: Some(addr),
        };

        cmd.write_to(&mut send).await?;
        send.write_all(&pkt).await?;
        send.finish().await?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct IncomingPackets {
    uni_streams: IncomingUniStreams,
    datagrams: Datagrams,
    udp_relay_mode: UdpRelayMode,
    stream_reg: Arc<StreamReg>,
    pkt_buf: PacketBuffer,
    last_gc_time: Instant,
}

impl IncomingPackets {
    pub async fn accept(
        &mut self,
        gc_interval: Duration,
        gc_timeout: Duration,
    ) -> Option<Result<Packet, IncomingPacketsError>> {
        match self.udp_relay_mode {
            UdpRelayMode::Native => self.accept_from_datagrams(gc_interval, gc_timeout).await,
            UdpRelayMode::Quic => self.accept_from_uni_streams().await,
        }
    }

    async fn accept_from_datagrams(
        &mut self,
        gc_interval: Duration,
        gc_timeout: Duration,
    ) -> Option<Result<Packet, IncomingPacketsError>> {
        async fn process_datagram(
            pkt_buf: &mut PacketBuffer,
            dg: Result<Bytes, QuinnConnectionError>,
        ) -> Result<Option<Packet>, IncomingPacketsError> {
            let dg = dg.unwrap();
            let cmd = Command::read_from(&mut dg.as_ref()).await.unwrap();
            let cmd_len = cmd.serialized_len();

            match cmd {
                Command::Packet {
                    assoc_id,
                    pkt_id,
                    frag_total,
                    frag_id,
                    len,
                    addr,
                } => {
                    if let Some(pkt) = pkt_buf.insert(
                        assoc_id,
                        pkt_id,
                        frag_total,
                        frag_id,
                        addr,
                        dg.slice(cmd_len..cmd_len + len as usize),
                    )? {
                        Ok(Some(pkt))
                    } else {
                        Ok(None)
                    }
                }
                cmd => Err(IncomingPacketsError::Tuic(TuicError::InvalidCommand(
                    cmd.as_type_code(),
                ))),
            }
        }

        if self.last_gc_time.elapsed() > gc_interval {
            self.pkt_buf.collect_garbage(gc_timeout);
            self.last_gc_time = Instant::now();
        }

        let mut gc_interval = time::interval(gc_interval);

        loop {
            tokio::select! {
                dg = self.datagrams.next() => {
                    if let Some(dg) = dg {
                        match process_datagram(&mut self.pkt_buf, dg).await {
                            Ok(Some(pkt)) => break Some(Ok(pkt)),
                            Ok(None) => {}
                            Err(err) => break Some(Err(err)),
                        }
                    } else {
                        break None;
                    }
                }
                _ = gc_interval.tick() => {
                    self.pkt_buf.collect_garbage(gc_timeout);
                    self.last_gc_time = Instant::now();
                }
            }
        }
    }

    async fn accept_from_uni_streams(&mut self) -> Option<Result<Packet, IncomingPacketsError>> {
        async fn process_uni_stream(
            recv: Result<QuinnRecvStream, QuinnConnectionError>,
            stream_reg: StreamReg,
        ) -> Result<Packet, IncomingPacketsError> {
            let recv = match recv {
                Ok(recv) => recv,
                Err(err) => return Err(IncomingPacketsError::from_quinn_connection_error(err)),
            };

            let mut recv = RecvStream::new(recv, stream_reg);
            let cmd = Command::read_from(&mut recv).await?;

            match cmd {
                Command::Packet {
                    assoc_id,
                    pkt_id,
                    frag_total,
                    frag_id,
                    len,
                    addr,
                } => {
                    if frag_id != 0 || frag_total != 1 {
                        return Err(IncomingPacketsError::FragmentedPacketFromUniStream);
                    }

                    if addr.is_none() {
                        return Err(IncomingPacketsError::NoAddressPacketFromUniStream);
                    }

                    let mut buf = vec![0; len as usize];
                    recv.read_exact(&mut buf).await?;
                    let pkt = Bytes::from(buf);

                    Ok(Packet::new(assoc_id, pkt_id, addr.unwrap(), pkt))
                }
                _ => Err(IncomingPacketsError::Tuic(TuicError::InvalidCommand(
                    cmd.as_type_code(),
                ))),
            }
        }

        if let Some(recv) = self.uni_streams.next().await {
            Some(process_uni_stream(recv, self.stream_reg.as_ref().clone()).await)
        } else {
            None
        }
    }
}

#[derive(Error, Debug)]
pub enum IncomingPacketsError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Tuic(TuicError),
    #[error(transparent)]
    PacketBuffer(#[from] PacketBufferError),
    #[error("received fragmented packet from uni stream")]
    FragmentedPacketFromUniStream,
    #[error("received packet without address from uni stream")]
    NoAddressPacketFromUniStream,
}

impl IncomingPacketsError {
    #[inline]
    fn from_quinn_connection_error(err: QuinnConnectionError) -> Self {
        Self::Io(IoError::from(err))
    }
}

impl From<TuicError> for IncomingPacketsError {
    #[inline]
    fn from(err: TuicError) -> Self {
        match err {
            TuicError::Io(err) => Self::Io(err),
            err => Self::Tuic(err),
        }
    }
}

impl From<IncomingPacketsError> for IoError {
    #[inline]
    fn from(err: IncomingPacketsError) -> Self {
        match err {
            IncomingPacketsError::Io(err) => Self::from(err),
            err => Self::new(ErrorKind::Other, err),
        }
    }
}
