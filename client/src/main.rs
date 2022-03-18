#![feature(once_cell)]
#![feature(try_blocks)]

use crate::{config::ConfigBuilder, relay::Relay};
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

    let socks5_server = match socks5::init(config.local_addr, config.socks5_auth, req_tx).await {
        Ok(res) => res,
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    };

    let _ = tokio::join!(relay, socks5_server);
}
