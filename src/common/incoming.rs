use super::stream::{RecvStream, SendStream, Stream as BiStream, StreamReg};
use bytes::Bytes;
use futures::{stream::SelectAll, Stream};
use quinn::{
    ConnectionError as QuinnConnectionError, Datagrams, IncomingBiStreams, IncomingUniStreams,
    RecvStream as QuinnRecvStream, SendStream as QuinnSendStream,
};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

pub(crate) struct IncomingTasks {
    incoming: SelectAll<IncomingSource>,
    stream_reg: Arc<StreamReg>,
}

impl IncomingTasks {
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

impl Stream for IncomingTasks {
    type Item = Result<PendingTask, QuinnConnectionError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.incoming)
            .poll_next(cx)
            .map_ok(|src| match src {
                IncomingItem::BiStream((send, recv)) => PendingTask::BiStream(BiStream::new(
                    SendStream::new(send, self.stream_reg.as_ref().clone()),
                    RecvStream::new(recv, self.stream_reg.as_ref().clone()),
                )),
                IncomingItem::UniStream(recv) => {
                    PendingTask::UniStream(RecvStream::new(recv, self.stream_reg.as_ref().clone()))
                }
                IncomingItem::Datagram(datagram) => PendingTask::Datagram(datagram),
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

pub(crate) enum PendingTask {
    BiStream(BiStream),
    UniStream(RecvStream),
    Datagram(Bytes),
}
