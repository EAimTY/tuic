use crate::{config::CongestionController, connection::Connection};
use anyhow::Result;
use futures_util::StreamExt;
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    Endpoint, Incoming, ServerConfig, TransportConfig,
};
use rustls::{Certificate, PrivateKey};
use std::{net::SocketAddr, sync::Arc};

pub struct Server {
    incoming: Incoming,
    expected_token_digest: [u8; 32],
}

impl Server {
    pub fn init(
        port: u16,
        expected_token_digest: [u8; 32],
        cert: Certificate,
        priv_key: PrivateKey,
        congestion_controller: CongestionController,
    ) -> Result<Self> {
        let config = {
            let mut config = ServerConfig::with_single_cert(vec![cert], priv_key)?;

            let mut transport = TransportConfig::default();

            match congestion_controller {
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

        let (_, incoming) = Endpoint::server(config, SocketAddr::from(([0, 0, 0, 0], port)))?;

        Ok(Self {
            incoming,
            expected_token_digest,
        })
    }

    pub async fn run(mut self) {
        while let Some(conn) = self.incoming.next().await {
            tokio::spawn(Connection::handle_connection(
                conn,
                self.expected_token_digest,
            ));
        }
    }
}
