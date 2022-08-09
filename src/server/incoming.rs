use crate::UdpRelayMode;
use crossbeam_utils::atomic::AtomicCell;
use quinn::{Datagrams, IncomingUniStreams};
use std::sync::Arc;

pub struct IncomingTasks {
    uni_streams: IncomingUniStreams,
    datagrams: Datagrams,
    udp_relay_mode: Arc<AtomicCell<Option<UdpRelayMode>>>,
}

impl IncomingTasks {
    pub(super) fn new(
        uni_streams: IncomingUniStreams,
        datagrams: Datagrams,
        udp_relay_mode: Arc<AtomicCell<Option<UdpRelayMode>>>,
    ) -> Self {
        Self {
            uni_streams,
            datagrams,
            udp_relay_mode,
        }
    }
}
