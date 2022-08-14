use crate::{
    common::{
        stream::{RecvStream, StreamReg},
        util::{PacketBuffer, PacketBufferError},
    },
    protocol::{Command, Error as TuicError},
    task::Packet,
    PacketBufferGcHandle, UdpRelayMode,
};
use bytes::Bytes;
use futures_util::StreamExt;
use quinn::{
    ConnectionError as QuinnConnectionError, Datagrams, IncomingUniStreams,
    RecvStream as QuinnRecvStream,
};
use std::{
    io::{Error as IoError, ErrorKind},
    sync::Arc,
};
use thiserror::Error;
use tokio::io::AsyncReadExt;

#[derive(Debug)]
pub struct Incoming {
    uni_streams: IncomingUniStreams,
    datagrams: Datagrams,
    udp_relay_mode: UdpRelayMode,
    stream_reg: Arc<StreamReg>,
    pkt_buf: PacketBuffer,
}

impl Incoming {
    pub(super) fn new(
        uni_streams: IncomingUniStreams,
        datagrams: Datagrams,
        udp_relay_mode: UdpRelayMode,
        stream_reg: Arc<StreamReg>,
    ) -> Self {
        Self {
            uni_streams,
            datagrams,
            udp_relay_mode,
            stream_reg,
            pkt_buf: PacketBuffer::new(),
        }
    }

    pub async fn accept(&mut self) -> Option<Result<PendingTask, IncomingError>> {
        let handle_raw_datagram = |dg: Result<Bytes, QuinnConnectionError>| {
            dg.map(|dg| {
                PendingTask::new(
                    TaskSource::Datagram(dg),
                    self.udp_relay_mode,
                    self.pkt_buf.clone(),
                )
            })
            .map_err(IncomingError::from_quinn_connection_error)
        };

        let handle_raw_uni_stream = |recv: Result<QuinnRecvStream, QuinnConnectionError>| {
            recv.map(|recv| {
                PendingTask::new(
                    TaskSource::RecvStream(RecvStream::new(recv, self.stream_reg.as_ref().clone())),
                    self.udp_relay_mode,
                    self.pkt_buf.clone(),
                )
            })
            .map_err(IncomingError::from_quinn_connection_error)
        };

        tokio::select! {
            dg = self.datagrams.next() => if let Some(dg) = dg {
                    Some(handle_raw_datagram(dg))
                } else {
                    self.uni_streams.next().await.map(handle_raw_uni_stream)
                },
            recv = self.uni_streams.next() => if let Some(recv) = recv {
                    Some(handle_raw_uni_stream(recv))
                } else {
                    self.datagrams.next().await.map(handle_raw_datagram)
                },
        }
    }

    pub fn get_packet_buffer_gc_handler(&self) -> PacketBufferGcHandle {
        self.pkt_buf.get_gc_handler()
    }
}

#[derive(Debug)]
pub struct PendingTask {
    source: TaskSource,
    udp_relay_mode: UdpRelayMode,
    pkt_buf: PacketBuffer,
}

impl PendingTask {
    fn new(source: TaskSource, udp_relay_mode: UdpRelayMode, pkt_buf: PacketBuffer) -> Self {
        Self {
            source,
            udp_relay_mode,
            pkt_buf,
        }
    }

    pub async fn parse(self) -> Result<Task, IncomingError> {
        match self.source {
            TaskSource::Datagram(dg) => {
                Self::parse_datagram(dg, self.udp_relay_mode, self.pkt_buf).await
            }
            TaskSource::RecvStream(recv) => {
                Self::parse_recv_stream(recv, self.udp_relay_mode).await
            }
        }
    }

    #[inline]
    async fn parse_datagram(
        dg: Bytes,
        udp_relay_mode: UdpRelayMode,
        mut pkt_buf: PacketBuffer,
    ) -> Result<Task, IncomingError> {
        let cmd = Command::read_from(&mut dg.as_ref()).await?;
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
                if !matches!(udp_relay_mode, UdpRelayMode::Native) {
                    return Err(IncomingError::BadCommand(Command::TYPE_PACKET, "datagram"));
                }

                if let Some(pkt) = pkt_buf.insert(
                    assoc_id,
                    pkt_id,
                    frag_total,
                    frag_id,
                    addr,
                    dg.slice(cmd_len..cmd_len + len as usize),
                )? {
                    Ok(Task::Packet(Some(pkt)))
                } else {
                    Ok(Task::Packet(None))
                }
            }
            cmd => Err(IncomingError::BadCommand(cmd.as_type_code(), "datagram")),
        }
    }

    #[inline]
    async fn parse_recv_stream(
        mut recv: RecvStream,
        udp_relay_mode: UdpRelayMode,
    ) -> Result<Task, IncomingError> {
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
                if !matches!(udp_relay_mode, UdpRelayMode::Quic) {
                    return Err(IncomingError::BadCommand(
                        Command::TYPE_PACKET,
                        "uni_stream",
                    ));
                }

                if frag_id != 0 || frag_total != 1 {
                    return Err(IncomingError::BadFragment);
                }

                if addr.is_none() {
                    return Err(IncomingError::NoAddress);
                }

                let mut buf = vec![0; len as usize];
                recv.read_exact(&mut buf).await?;
                let pkt = Bytes::from(buf);

                Ok(Task::Packet(Some(Packet::new(
                    assoc_id,
                    pkt_id,
                    addr.unwrap(),
                    pkt,
                ))))
            }
            _ => Err(IncomingError::BadCommand(cmd.as_type_code(), "uni_stream")),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Task {
    Packet(Option<Packet>),
}

#[derive(Debug)]
enum TaskSource {
    Datagram(Bytes),
    RecvStream(RecvStream),
}

#[derive(Error, Debug)]
pub enum IncomingError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Tuic(TuicError),
    #[error("received bad command {0:#x} from {1}")]
    BadCommand(u8, &'static str),
    #[error("received bad-fragmented packet")]
    BadFragment,
    #[error("missing address in packet with frag_id 0")]
    NoAddress,
    #[error("unexpected address in packet")]
    UnexpectedAddress,
}

impl IncomingError {
    #[inline]
    fn from_quinn_connection_error(err: QuinnConnectionError) -> Self {
        Self::Io(IoError::from(err))
    }
}

impl From<PacketBufferError> for IncomingError {
    #[inline]
    fn from(err: PacketBufferError) -> Self {
        match err {
            PacketBufferError::NoAddress => Self::NoAddress,
            PacketBufferError::UnexpectedAddress => Self::UnexpectedAddress,
            PacketBufferError::BadFragment => Self::BadFragment,
        }
    }
}

impl From<TuicError> for IncomingError {
    #[inline]
    fn from(err: TuicError) -> Self {
        match err {
            TuicError::Io(err) => Self::Io(err),
            err => Self::Tuic(err),
        }
    }
}

impl From<IncomingError> for IoError {
    #[inline]
    fn from(err: IncomingError) -> Self {
        match err {
            IncomingError::Io(err) => Self::from(err),
            err => Self::new(ErrorKind::Other, err),
        }
    }
}
