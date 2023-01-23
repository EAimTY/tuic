use self::authenticate::Method as AuthenticationMethod;

mod address;
mod connect;
mod dissociate;
mod heartbeat;
mod packet;

pub mod authenticate;

pub use self::{
    address::Address, authenticate::Authenticate, connect::Connect, dissociate::Dissociate,
    heartbeat::Heartbeat, packet::Packet,
};

pub const VERSION: u8 = 0x05;

/// Header
///
/// ```plain
/// +-----+----------+----------+
/// | VER |   TYPE   |   OPT    |
/// +-----+----------+----------+
/// |  1  |    1     | Variable |
/// +-----+----------+----------+
/// ```
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Header<A>
where
    A: AuthenticationMethod,
{
    Authenticate(Authenticate<A>),
    Connect(Connect),
    Packet(Packet),
    Dissociate(Dissociate),
    Heartbeat(Heartbeat),
}

impl<A> Header<A>
where
    A: AuthenticationMethod,
{
    pub const TYPE_CODE_AUTHENTICATE: u8 = Authenticate::<A>::TYPE_CODE;
    pub const TYPE_CODE_CONNECT: u8 = Connect::TYPE_CODE;
    pub const TYPE_CODE_PACKET: u8 = Packet::TYPE_CODE;
    pub const TYPE_CODE_DISSOCIATE: u8 = Dissociate::TYPE_CODE;
    pub const TYPE_CODE_HEARTBEAT: u8 = Heartbeat::TYPE_CODE;

    pub fn type_code(&self) -> u8 {
        match self {
            Self::Authenticate(_) => Authenticate::<A>::type_code(),
            Self::Connect(_) => Connect::type_code(),
            Self::Packet(_) => Packet::type_code(),
            Self::Dissociate(_) => Dissociate::type_code(),
            Self::Heartbeat(_) => Heartbeat::type_code(),
        }
    }

    pub fn len(&self) -> usize {
        2 + match self {
            Self::Authenticate(auth) => auth.len(),
            Self::Connect(connect) => connect.len(),
            Self::Packet(packet) => packet.len(),
            Self::Dissociate(dissociate) => dissociate.len(),
            Self::Heartbeat(heartbeat) => heartbeat.len(),
        }
    }
}

pub trait Command {
    fn type_code() -> u8;
    fn len(&self) -> usize;
}
