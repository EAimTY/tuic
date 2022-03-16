use crate::{config::CongestionController, connection::Connection};
use anyhow::Result;
use futures_util::StreamExt;
use quinn::{
    congestion::{BbrConfig, CubicConfig, NewRenoConfig},
    Endpoint, Incoming, ServerConfig, TransportConfig,
};
use rustls::{Certificate, PrivateKey};
use std::{net::SocketAddr, sync::Arc, time::Duration};

pub struct Server {
    incoming: Incoming,
    expected_token_digest: [u8; 32],
    authentication_timeout: Duration,
}

impl Server {
    pub fn init(
        port: u16,
        exp_tkn_dgst: [u8; 32],
        cert: Certificate,
        priv_key: PrivateKey,
        auth_timeout: Duration,
        cgstn_ctrl: CongestionController,
    ) -> Result<Self> {
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

        let (_, incoming) = Endpoint::server(config, SocketAddr::from(([0, 0, 0, 0], port)))?;

        Ok(Self {
            incoming,
            expected_token_digest: exp_tkn_dgst,
            authentication_timeout: auth_timeout,
        })
    }

    pub async fn run(mut self) {
        while let Some(conn) = self.incoming.next().await {
            tokio::spawn(Connection::handle(
                conn,
                self.expected_token_digest,
                self.authentication_timeout,
            ));
        }
    }
}
