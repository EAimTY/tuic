pub(crate) mod stream;
pub mod task;
pub(crate) mod util;

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
