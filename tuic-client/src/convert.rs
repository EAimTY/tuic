use crate::socks5;

impl From<socks5::ConnectRequest> for tuic_protocol::ConnectRequest {
    fn from(req: socks5::ConnectRequest) -> Self {
        tuic_protocol::ConnectRequest {
            address: req.address.into(),
            command: req.command.into(),
        }
    }
}

impl From<socks5::Address> for tuic_protocol::Address {
    fn from(addr: socks5::Address) -> Self {
        match addr {
            socks5::Address::SocketAddress(addr) => Self::SocketAddress(addr),
            socks5::Address::UriAuthorityAddress(authority, port) => {
                Self::UriAuthorityAddress(authority, port)
            }
        }
    }
}

impl From<socks5::Command> for tuic_protocol::Command {
    fn from(cmd: socks5::Command) -> Self {
        match cmd {
            socks5::Command::Connect => Self::Connect,
        }
    }
}
