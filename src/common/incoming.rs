use super::{
    stream::{RecvStream, SendStream, Stream as BiStream, StreamReg},
    task::{RawTask, RawTaskPayload},
};
use crate::protocol::{Command, Error as ProtocalError};
use bytes::Bytes;
use futures::{stream::SelectAll, Stream};
use quinn::{
    ConnectionError as QuinnConnectionError, Datagrams, IncomingBiStreams, IncomingUniStreams,
    RecvStream as QuinnRecvStream, SendStream as QuinnSendStream,
};
use std::{
    io::Error as IoError,
    pin::Pin,
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
    type Item = Result<RawPendingTask, QuinnConnectionError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.incoming)
            .poll_next(cx)
            .map_ok(|src| match src {
                IncomingItem::BiStream((send, recv)) => RawPendingTask::BiStream(BiStream::new(
                    SendStream::new(send, self.stream_reg.as_ref().clone()),
                    RecvStream::new(recv, self.stream_reg.as_ref().clone()),
                )),
                IncomingItem::UniStream(recv) => RawPendingTask::UniStream(RecvStream::new(
                    recv,
                    self.stream_reg.as_ref().clone(),
                )),
                IncomingItem::Datagram(datagram) => RawPendingTask::Datagram(datagram),
            })
    }
}

enum IncomingSource {
    BiStreams(IncomingBiStreams),
    UniStreams(IncomingUniStreams),
    Datagrams(Datagrams),
}

impl Stream for IncomingSource {
    type Item = Result<IncomingItem, QuinnConnectionError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            IncomingSource::BiStreams(bi_streams) => Pin::new(bi_streams)
                .poll_next(cx)
                .map_ok(IncomingItem::BiStream),
            IncomingSource::UniStreams(uni_streams) => Pin::new(uni_streams)
                .poll_next(cx)
                .map_ok(IncomingItem::UniStream),
            IncomingSource::Datagrams(datagrams) => Pin::new(datagrams)
                .poll_next(cx)
                .map_ok(IncomingItem::Datagram),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

enum IncomingItem {
    BiStream((QuinnSendStream, QuinnRecvStream)),
    UniStream(QuinnRecvStream),
    Datagram(Bytes),
}

pub(crate) enum RawPendingTask {
    BiStream(BiStream),
    UniStream(RecvStream),
    Datagram(Bytes),
}

impl RawPendingTask {
    pub(crate) async fn accept(self) -> RawTask {
        match self {
            RawPendingTask::BiStream(mut bi_stream) => RawTask::new(
                Command::read_from(&mut bi_stream).await.unwrap(),
                RawTaskPayload::BiStream(bi_stream),
            ),
            RawPendingTask::UniStream(mut uni_stream) => RawTask::new(
                Command::read_from(&mut uni_stream).await.unwrap(),
                RawTaskPayload::UniStream(uni_stream),
            ),
            RawPendingTask::Datagram(datagram) => {
                let cmd = Command::read_from(&mut datagram.as_ref()).await.unwrap();
                let payload = datagram.slice(cmd.serialized_len()..);
                RawTask::new(cmd, RawTaskPayload::Datagram(payload))
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum IncomingError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Protocol(ProtocalError),
}
