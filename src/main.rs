use crate::config::{Config, ConfigBuilder};
use std::env;

mod certificate;
mod client;
mod config;
mod server;
mod socks5;

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

    match config {
        Config::Client(cfg) => {
            if let Err(err) = client::start(cfg).await {
                eprintln!("{}", err);
            }
        }
        Config::Server(cfg) => {
            if let Err(err) = server::start(cfg).await {
                eprintln!("{}", err);
            }
        }
    }
}
