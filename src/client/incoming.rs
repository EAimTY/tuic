use super::ConnectionError;
use crate::{
    common::{incoming::RawIncomingTasks, stream::StreamReg, task::TaskSource, util::PacketBuffer},
    Packet, PacketBufferGcHandle, UdpRelayMode,
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
    type Item = Result<PendingTask, ConnectionError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx).map(|poll| {
            poll.map(|res| match res {
                Ok(source) => match (source, self.udp_relay_mode) {
                    (TaskSource::BiStream(stream), _) => {
                        Err(ConnectionError::UnexpectedIncomingBiStream(stream))
                    }
                    (TaskSource::UniStream(stream), UdpRelayMode::Native) => {
                        Err(ConnectionError::UnexpectedIncomingUniStream(stream))
                    }
                    (TaskSource::Datagram(datagram), UdpRelayMode::Quic) => {
                        Err(ConnectionError::UnexpectedIncomingDatagram(datagram))
                    }
                    (source, _) => Ok(PendingTask::new(source, self.pkt_buf.clone())),
                },
                Err(err) => Err(ConnectionError::from(err)),
            })
        })
    }
}

pub struct PendingTask {
    inner: TaskSource,
    pkt_buf: PacketBuffer,
}

impl PendingTask {
    fn new(inner: TaskSource, pkt_buf: PacketBuffer) -> Self {
        Self { inner, pkt_buf }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Task {
    Packet(Option<Packet>),
}
