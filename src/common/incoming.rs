use super::stream::{BiStream, RecvStream, SendStream, StreamReg};
use crate::protocol::{Address, Command, MarshalingError, ProtocolError};
use bytes::Bytes;
use futures::{stream::SelectAll, Stream};
use quinn::{
    Datagrams, IncomingBiStreams, IncomingUniStreams, RecvStream as QuinnRecvStream,
    SendStream as QuinnSendStream,
};
use std::{
    io::Error as IoError,
    pin::Pin,
    string::FromUtf8Error,
    sync::Arc,
    task::{Context, Poll},
};
use thiserror::Error;

pub(crate) struct RawIncomingTasks {
    incoming: SelectAll<IncomingSource>,
    stream_reg: Arc<StreamReg>,
}

impl RawIncomingTasks {
    pub(crate) fn new(
        bi_streams: IncomingBiStreams,
        uni_streams: IncomingUniStreams,
        datagrams: Datagrams,
        stream_reg: Arc<StreamReg>,
    ) -> Self {
        let mut incoming = SelectAll::new();

        incoming.push(IncomingSource::BiStreams(bi_streams));
        incoming.push(IncomingSource::UniStreams(uni_streams));
        incoming.push(IncomingSource::Datagrams(datagrams));

        Self {
            incoming,
            stream_reg,
        }
    }
}

impl Stream for RawIncomingTasks {
    type Item = Result<RawPendingIncomingTask, IoError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.incoming)
            .poll_next(cx)
            .map_ok(|src| match src {
                IncomingItem::BiStream((send, recv)) => {
                    RawPendingIncomingTask::BiStream(BiStream::new(
                        SendStream::new(send, self.stream_reg.as_ref().clone()),
                        RecvStream::new(recv, self.stream_reg.as_ref().clone()),
                    ))
                }
                IncomingItem::UniStream(recv) => RawPendingIncomingTask::UniStream(
                    RecvStream::new(recv, self.stream_reg.as_ref().clone()),
                ),
                IncomingItem::Datagram(datagram) => RawPendingIncomingTask::Datagram(datagram),
            })
            .map_err(IoError::from)
    }
}

enum IncomingSource {
    BiStreams(IncomingBiStreams),
    UniStreams(IncomingUniStreams),
    Datagrams(Datagrams),
}

impl Stream for IncomingSource {
    type Item = Result<IncomingItem, IoError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            IncomingSource::BiStreams(bi_streams) => Pin::new(bi_streams)
                .poll_next(cx)
                .map_ok(IncomingItem::BiStream)
                .map_err(IoError::from),
            IncomingSource::UniStreams(uni_streams) => Pin::new(uni_streams)
                .poll_next(cx)
                .map_ok(IncomingItem::UniStream)
                .map_err(IoError::from),
            IncomingSource::Datagrams(datagrams) => Pin::new(datagrams)
                .poll_next(cx)
                .map_ok(IncomingItem::Datagram)
                .map_err(IoError::from),
        }
    }
}

enum IncomingItem {
    BiStream((QuinnSendStream, QuinnRecvStream)),
    UniStream(QuinnRecvStream),
    Datagram(Bytes),
}

pub(crate) enum RawPendingIncomingTask {
    BiStream(BiStream),
    UniStream(RecvStream),
    Datagram(Bytes),
}

impl RawPendingIncomingTask {
    pub(crate) async fn accept(self) -> Result<RawIncomingTask, IncomingError> {
        match self {
            Self::BiStream(stream) => Self::accept_from_bi_stream(stream).await,
            Self::UniStream(stream) => Self::accept_from_uni_stream(stream).await,
            Self::Datagram(datagram) => Self::accept_from_datagram(datagram).await,
        }
    }

    async fn accept_from_bi_stream(mut stream: BiStream) -> Result<RawIncomingTask, IncomingError> {
        let cmd = Command::read_from(&mut stream)
            .await
            .map_err(IncomingError::from_marshaling_error)?;

        match cmd {
            Command::Connect { addr } => Ok(RawIncomingTask::Connect {
                addr,
                payload: stream,
            }),
            cmd => Err(IncomingError::UnexpectedCommandFromBiStream(stream, cmd)),
        }
    }

    async fn accept_from_uni_stream(
        mut stream: RecvStream,
    ) -> Result<RawIncomingTask, IncomingError> {
        let cmd = Command::read_from(&mut stream)
            .await
            .map_err(IncomingError::from_marshaling_error)?;

        match cmd {
            Command::Authenticate(token) => Ok(RawIncomingTask::Authenticate { token }),
            Command::Packet {
                assoc_id,
                pkt_id,
                frag_total,
                frag_id,
                len,
                addr,
            } => Ok(RawIncomingTask::PacketFromUniStream {
                assoc_id,
                pkt_id,
                frag_total,
                frag_id,
                len,
                addr,
                payload: stream,
            }),
            Command::Dissociate { assoc_id } => Ok(RawIncomingTask::Dissociate { assoc_id }),
            Command::Heartbeat => Ok(RawIncomingTask::Heartbeat),
            cmd => Err(IncomingError::UnexpectedCommandFromUniStream(stream, cmd)),
        }
    }

    async fn accept_from_datagram(datagram: Bytes) -> Result<RawIncomingTask, IncomingError> {
        let cmd = Command::read_from(&mut datagram.as_ref())
            .await
            .map_err(IncomingError::from_marshaling_error)?;
        let payload = datagram.slice(cmd.serialized_len()..);

        match cmd {
            Command::Packet {
                assoc_id,
                pkt_id,
                frag_total,
                frag_id,
                len,
                addr,
            } => Ok(RawIncomingTask::PacketFromDatagram {
                assoc_id,
                pkt_id,
                frag_total,
                frag_id,
                len,
                addr,
                payload: datagram,
            }),
            cmd => Err(IncomingError::UnexpectedCommandFromDatagram(payload, cmd)),
        }
    }
}

#[non_exhaustive]
pub(crate) enum RawIncomingTask {
    Authenticate {
        token: [u8; 32],
    },
    Connect {
        addr: Address,
        payload: BiStream,
    },
    PacketFromDatagram {
        assoc_id: u32,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        len: u16,
        addr: Option<Address>,
        payload: Bytes,
    },
    PacketFromUniStream {
        assoc_id: u32,
        pkt_id: u16,
        frag_total: u8,
        frag_id: u8,
        len: u16,
        addr: Option<Address>,
        payload: RecvStream,
    },
    Dissociate {
        assoc_id: u32,
    },
    Heartbeat,
}

#[derive(Error, Debug)]
pub enum IncomingError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("invalid address encoding: {0}")]
    InvalidEncoding(#[from] FromUtf8Error),
    #[error("unexpected incoming bi_stream")]
    UnexpectedIncomingBiStream(BiStream),
    #[error("unexpected incoming uni_stream")]
    UnexpectedIncomingUniStream(RecvStream),
    #[error("unexpected incoming datagram")]
    UnexpectedIncomingDatagram(Bytes),
    #[error("unexpected command from bi_stream: {1:?}")]
    UnexpectedCommandFromBiStream(BiStream, Command),
    #[error("unexpected command from uni_stream: {1:?}")]
    UnexpectedCommandFromUniStream(RecvStream, Command),
    #[error("unexpected command from datagram: {1:?}")]
    UnexpectedCommandFromDatagram(Bytes, Command),
}

impl IncomingError {
    #[inline]
    pub(super) fn from_marshaling_error(err: MarshalingError) -> Self {
        match err {
            MarshalingError::Io(err) => Self::Io(err),
            MarshalingError::Protocol(err) => Self::Protocol(err),
            MarshalingError::InvalidEncoding(err) => Self::InvalidEncoding(err),
        }
    }
}
