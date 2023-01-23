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

/// Command
///
/// ```plain
/// +-----+----------+----------+
/// | VER | CMD_TYPE |   OPT    |
/// +-----+----------+----------+
/// |  1  |    1     | Variable |
/// +-----+----------+----------+
/// ```
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Command<A>
where
    A: AuthenticationMethod,
{
    Authenticate(Authenticate<A>),
    Connect(Connect),
    Packet(Packet),
    Dissociate(Dissociate),
    Heartbeat(Heartbeat),
}

impl<A> Command<A>
where
    A: AuthenticationMethod,
{
    pub fn cmd_type(&self) -> u8 {
        match self {
            Self::Authenticate(_) => Authenticate::<A>::cmd_type(),
            Self::Connect(_) => Connect::cmd_type(),
            Self::Packet(_) => Packet::cmd_type(),
            Self::Dissociate(_) => Dissociate::cmd_type(),
            Self::Heartbeat(_) => Heartbeat::cmd_type(),
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
