pub(crate) mod stream;
pub(crate) mod util;

pub mod task;

pub use self::util::PacketBufferGcHandle;

#[derive(Clone, Copy, Debug)]
pub enum CongestionControl {
    Cubic,
    NewReno,
    Bbr,
}

#[derive(Clone, Copy, Debug)]
pub enum UdpRelayMode {
    Native,
    Quic,
}
