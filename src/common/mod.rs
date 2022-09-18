pub(crate) mod incoming;
pub(crate) mod packet;
pub(crate) mod stream;

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
