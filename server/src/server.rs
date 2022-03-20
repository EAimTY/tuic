use crate::connection::Connection;
use futures_util::StreamExt;
use quinn::{Endpoint, Incoming, ServerConfig};
use std::{io::Error as IoError, net::SocketAddr, time::Duration};

pub struct Server {
    incoming: Incoming,
    socket_addr: SocketAddr,
    expected_token_digest: [u8; 32],
    authentication_timeout: Duration,
    max_udp_packet_size: usize,
}

impl Server {
    pub fn init(
        config: ServerConfig,
        port: u16,
        exp_tkn_dgst: [u8; 32],
        auth_timeout: Duration,
        max_udp_pkt_size: usize,
    ) -> Result<Self, IoError> {
        let (endpoint, incoming) =
            Endpoint::server(config, SocketAddr::from(([0, 0, 0, 0], port)))?;

        let socket_addr = endpoint.local_addr()?;

        Ok(Self {
            incoming,
            socket_addr,
            expected_token_digest: exp_tkn_dgst,
            authentication_timeout: auth_timeout,
            max_udp_packet_size: max_udp_pkt_size,
        })
    }

    pub async fn run(mut self) {
        log::info!("Server started. Listening: {}", self.socket_addr);

        while let Some(conn) = self.incoming.next().await {
            tokio::spawn(Connection::handle(
                conn,
                self.expected_token_digest,
                self.authentication_timeout,
                self.max_udp_packet_size,
            ));
        }
    }
}
