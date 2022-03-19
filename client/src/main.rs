#![feature(once_cell)]

use crate::{config::ConfigBuilder, relay::Relay, socks5::Socks5};
use std::{env, process};

mod certificate;
mod config;
mod relay;
mod socks5;

#[tokio::main]
async fn main() {
    let args = env::args().collect::<Vec<_>>();

    let mut cfg_builder = ConfigBuilder::new();

    let config = match cfg_builder.parse(&args) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    };

    env_logger::builder()
        .filter_level(config.log_level)
        .format_level(true)
        .format_target(false)
        .format_module_path(false)
        .init();

    let (relay, req_tx) = match Relay::init(
        config.server_addr,
        config.certificate,
        config.token_digest,
        config.udp_mode,
        config.congestion_controller,
        config.reduce_rtt,
    ) {
        Ok((relay, tx)) => (tokio::spawn(relay.run()), tx),
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    };

    let socks5_server =
        match Socks5::init(config.local_addr, config.socks5_authentication, req_tx).await {
            Ok(socks5) => tokio::spawn(socks5.run()),
            Err(err) => {
                eprintln!("{err}");
                process::exit(1);
            }
        };

    let _ = tokio::join!(relay, socks5_server);
}
