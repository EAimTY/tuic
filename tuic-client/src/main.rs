use crate::config::ConfigBuilder;
use std::env;

mod certificate;
mod client;
mod config;
mod connection;
mod convert;
mod socks5_protocol;
mod socks5_server;

pub use crate::{
    config::Config,
    connection::{Connection, ConnectionManager},
    socks5_server::Socks5Server,
};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let mut cfg_builder = ConfigBuilder::new();

    let config = match cfg_builder.parse(&args) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("{}\n\n{}", err, cfg_builder.get_usage());
            return;
        }
    };

    match client::start(config).await {
        Ok(()) => {}
        Err(err) => eprintln!("{}", err),
    }
}
