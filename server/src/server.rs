use crate::connection::Connection;
use futures_util::{stream::SelectAll, StreamExt};
use quinn::{Endpoint, Incoming, ServerConfig};
use std::{
    io::Error as IoError,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

pub struct Server {
    incoming: SelectAll<Incoming>,
    port: u16,
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
        ipv4_only: bool,
        ipv6_only: bool,
    ) -> Result<Self, IoError> {
        let mut incoming = SelectAll::new();

        match (ipv4_only, ipv6_only) {
            (true, false) => {
                let (_, incoming_ipv4) =
                    Endpoint::server(config, SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))?;
                incoming.push(incoming_ipv4);
            }
            (false, true) => {
                let (_, incoming_ipv6) =
                    Endpoint::server(config, SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)))?;
                incoming.push(incoming_ipv6);
            }
            (true, true) => {
                let (_, incoming_ipv4) = Endpoint::server(
                    config.clone(),
                    SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)),
                )?;

                let (_, incoming_ipv6) =
                    Endpoint::server(config, SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)))?;

                incoming.push(incoming_ipv4);
                incoming.push(incoming_ipv6);
            }
            (false, false) => unreachable!(),
        }

        Ok(Self {
            incoming,
            port,
            expected_token_digest: exp_tkn_dgst,
            authentication_timeout: auth_timeout,
            max_udp_packet_size: max_udp_pkt_size,
        })
    }

    pub async fn run(mut self) {
        log::info!("Server started. Listening port: {}", self.port);

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
