use crate::{config::ConfigBuilder, relay::Relay, socks5::Socks5};
use std::{env, process};

mod certificate;
mod config;
mod relay;
mod socks5;

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

    let (relay, req_tx) = match Relay::init(
        config.config,
        config.server_addr,
        config.token_digest,
        config.udp_mode,
        config.reduce_rtt,
    ) {
        Ok((relay, tx)) => (tokio::spawn(relay.run()), tx),
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    let socks5 = match Socks5::init(
        config.local_addr,
        config.socks5_authentication,
        config.max_udp_packet_size,
        req_tx,
    )
    .await
    {
        Ok(socks5) => tokio::spawn(socks5.run()),
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    let _ = tokio::join!(relay, socks5);
    process::exit(1);
}
