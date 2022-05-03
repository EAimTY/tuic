use crate::connection::Connection;
use futures_util::StreamExt;
use quinn::{Endpoint, EndpointConfig, Incoming, ServerConfig};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    collections::HashSet,
    io::Error as IoError,
    net::{Ipv6Addr, SocketAddr, UdpSocket},
    sync::Arc,
    time::Duration,
};

pub struct Server {
    incoming: Incoming,
    port: u16,
    token: Arc<HashSet<[u8; 32]>>,
    authentication_timeout: Duration,
}

impl Server {
    pub fn init(
        config: ServerConfig,
        port: u16,
        token: HashSet<[u8; 32]>,
        auth_timeout: Duration,
    ) -> Result<Self, IoError> {
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_only_v6(false)?;
        socket.bind(&SockAddr::from(SocketAddr::from((
            Ipv6Addr::UNSPECIFIED,
            port,
        ))))?;
        let socket = UdpSocket::from(socket);

        let (_, incoming) = Endpoint::new(EndpointConfig::default(), Some(config), socket)?;

        Ok(Self {
            incoming,
            port,
            token: Arc::new(token),
            authentication_timeout: auth_timeout,
        })
    }

    pub async fn run(mut self) {
        log::info!("Server started. Listening port: {}", self.port);

        while let Some(conn) = self.incoming.next().await {
            let token = self.token.clone();

            tokio::spawn(Connection::handle(conn, token, self.authentication_timeout));
        }
    }
}
