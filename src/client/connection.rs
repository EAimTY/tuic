use quinn::{
    Connecting as QuinnConnecting, Connection as QuinnConnection, Datagrams, IncomingUniStreams,
};

use crate::UdpRelayMode;

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
}

#[derive(Debug)]
pub struct Connection {
    conn: QuinnConnection,
    token: [u8; 32],
    udp_relay_mode: UdpRelayMode,
}

impl Connection {
    pub(super) fn new(
        conn: QuinnConnection,
        token: [u8; 32],
        udp_relay_mode: UdpRelayMode,
    ) -> Self {
        Self {
            conn,
            token,
            udp_relay_mode,
        }
    }
}

#[derive(Debug)]
pub struct IncomingPackets {
    uni_streams: IncomingUniStreams,
    datagrams: Datagrams,
    udp_relay_mode: UdpRelayMode,
}

impl IncomingPackets {
    pub(super) fn new(
        uni_streams: IncomingUniStreams,
        datagrams: Datagrams,
        udp_relay_mode: UdpRelayMode,
    ) -> Self {
        Self {
            uni_streams,
            datagrams,
            udp_relay_mode,
        }
    }
}
