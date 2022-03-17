use crate::{config::CongestionController, connection::Connection};
use futures_util::StreamExt;
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    Endpoint, Incoming, ServerConfig, TransportConfig,
};
use rustls::Error as RustlsError;
use rustls::{Certificate, PrivateKey};
use std::{io::Error as IoError, net::SocketAddr, sync::Arc, time::Duration};
use thiserror::Error;

pub struct Server {
    incoming: Incoming,
    socket_addr: SocketAddr,
    expected_token_digest: [u8; 32],
    authentication_timeout: Duration,
    max_udp_packet_size: usize,
}

impl Server {
    pub fn init(
        port: u16,
        exp_tkn_dgst: [u8; 32],
        cert: Certificate,
        priv_key: PrivateKey,
        auth_timeout: Duration,
        cgstn_ctrl: CongestionController,
        max_udp_pkt_size: usize,
    ) -> Result<Self, ServerError> {
        let config = {
            let mut config = ServerConfig::with_single_cert(vec![cert], priv_key)?;

            let mut transport = TransportConfig::default();

            match cgstn_ctrl {
                CongestionController::Cubic => {
                    transport.congestion_controller_factory(Arc::new(CubicConfig::default()))
                }
                CongestionController::NewReno => {
                    transport.congestion_controller_factory(Arc::new(NewRenoConfig::default()))
                }
                CongestionController::Bbr => {
                    transport.congestion_controller_factory(Arc::new(BbrConfig::default()))
                }
            };

            config.transport = Arc::new(transport);

            config
        };

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

#[derive(Error, Debug)]
pub enum ServerError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Rustls(#[from] RustlsError),
}
