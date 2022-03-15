#![feature(try_blocks)]

use crate::{config::ConfigBuilder, server::Server};
use std::{env, process};

mod certificate;
mod config;
mod connection;
mod server;

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

    let server = match Server::init(
        config.port,
        config.token_digest,
        config.certificate,
        config.private_key,
        config.congestion_controller,
    ) {
        Ok(server) => server,
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    };

    server.run().await;
}
