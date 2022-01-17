use crate::{config::ConfigBuilder, connection::ConnectionGuard, socks5::Socks5Server};
use std::env;

mod certificate;
mod config;
mod connection;
mod error;
mod socks5;

pub use crate::{config::Config, error::ClientError};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let mut cfg_builder = ConfigBuilder::new();

    let config = match cfg_builder.parse(&args) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("{}", err);
            return;
        }
    };

    let (conn_guard, req_sender) = match ConnectionGuard::new(&config) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("{}", err);
            return;
        }
    };
    conn_guard.run().await;

    let socks5_server = Socks5Server::new(config, req_sender);

    match socks5_server.run().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("{}", err);
        }
    }
}
