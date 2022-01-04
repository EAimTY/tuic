use crate::socks5_protocol;

impl From<socks5_protocol::ConnectRequest> for tuic_protocol::ConnectRequest {
    fn from(req: socks5_protocol::ConnectRequest) -> Self {
        tuic_protocol::ConnectRequest {
            address: req.address.into(),
            command: req.command.into(),
        }
    }
}

impl From<socks5_protocol::Address> for tuic_protocol::Address {
    fn from(addr: socks5_protocol::Address) -> Self {
        match addr {
            socks5_protocol::Address::SocketAddress(addr) => Self::SocketAddress(addr),
            socks5_protocol::Address::UriAuthorityAddress(authority, port) => {
                Self::UriAuthorityAddress(authority, port)
            }
        }
    }
}

impl From<socks5_protocol::Command> for tuic_protocol::Command {
    fn from(cmd: socks5_protocol::Command) -> Self {
        match cmd {
            socks5_protocol::Command::Connect => Self::Connect,
        }
    }
}
