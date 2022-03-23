use crate::{config::ConfigBuilder, server::Server};
use std::{env, process};

mod certificate;
mod config;
mod connection;
mod server;

#[tokio::main]
async fn main() {
    let mut cfg_builder = ConfigBuilder::new();
    let args = env::args().collect::<Vec<_>>();

    let config = match cfg_builder.parse(&args) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    env_logger::builder()
        .filter_level(config.log_level)
        .format_level(true)
        .format_target(false)
        .format_module_path(false)
        .init();

    let server = match Server::init(
        config.config,
        config.port,
        config.token_digest,
        config.authentication_timeout,
        config.max_udp_packet_size,
        config.ipv6,
    ) {
        Ok(server) => server,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    server.run().await;
    process::exit(1);
}
