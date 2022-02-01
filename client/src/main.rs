use crate::{config::ConfigBuilder, connection::Connection, relay::Relay, socks5::Socks5Server};
use std::env;

mod certificate;
mod config;
mod connection;
mod error;
mod relay;
mod socks5;

pub use crate::{config::Config, error::ClientError};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let mut cfg_builder = ConfigBuilder::new();

    let config = match cfg_builder.parse(&args) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    if let Some(log_level) = config.log_level {
        match simple_logger::init_with_level(log_level) {
            Ok(()) => {}
            Err(err) => {
                eprintln!("Failed to initialize logger: {err}");
                return;
            }
        }
    }

    let (conn, stream_req_tx) = match Connection::init(&config) {
        Ok(res) => res,
        Err(err) => {
            log::error!("{err}");
            return;
        }
    };

    let (relay, relay_req_tx) = Relay::init(config.token, stream_req_tx);

    let socks5_server = match Socks5Server::init(config, relay_req_tx).await {
        Ok(res) => res,
        Err(err) => {
            log::error!("{err}");
            return;
        }
    };

    let _ = tokio::join!(conn, relay, socks5_server);
}
