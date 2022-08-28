use crate::{
    common::{
        incoming::{IncomingError, RawIncomingTasks, RawPendingIncomingTask},
        stream::StreamReg,
        util::PacketBuffer,
    },
    PacketBufferGcHandle, UdpRelayMode,
};
use futures::Stream;
use quinn::{Datagrams, IncomingBiStreams, IncomingUniStreams};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

pub struct IncomingTasks {
    inner: RawIncomingTasks,
    udp_relay_mode: UdpRelayMode,
    pkt_buf: PacketBuffer,
}

impl IncomingTasks {
    pub(super) fn new(
        bi_streams: IncomingBiStreams,
        uni_streams: IncomingUniStreams,
        datagrams: Datagrams,
        udp_relay_mode: UdpRelayMode,
        stream_reg: Arc<StreamReg>,
    ) -> Self {
        Self {
            inner: RawIncomingTasks::new(bi_streams, uni_streams, datagrams, stream_reg),
            udp_relay_mode,
            pkt_buf: PacketBuffer::new(),
        }
    }

    pub fn get_packet_buffer_gc_handler(&self) -> PacketBufferGcHandle {
        self.pkt_buf.get_gc_handler()
    }
}

impl Stream for IncomingTasks {
    type Item = Result<PendingIncomingTask, IncomingError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx).map(|poll| {
            poll.map(|res| match res {
                Ok(source) => match (source, self.udp_relay_mode) {
                    (RawPendingIncomingTask::BiStream(stream), _) => {
                        Err(IncomingError::UnexpectedIncomingBiStream(stream))
                    }
                    (RawPendingIncomingTask::UniStream(stream), UdpRelayMode::Native) => {
                        Err(IncomingError::UnexpectedIncomingUniStream(stream))
                    }
                    (RawPendingIncomingTask::Datagram(datagram), UdpRelayMode::Quic) => {
                        Err(IncomingError::UnexpectedIncomingDatagram(datagram))
                    }
                    (source, _) => Ok(PendingIncomingTask::new(source, self.pkt_buf.clone())),
                },
                Err(err) => Err(IncomingError::from(err)),
            })
        })
    }
}

pub struct PendingIncomingTask {
    inner: RawPendingIncomingTask,
    pkt_buf: PacketBuffer,
}

impl PendingIncomingTask {
    fn new(inner: RawPendingIncomingTask, pkt_buf: PacketBuffer) -> Self {
        Self { inner, pkt_buf }
    }
}
