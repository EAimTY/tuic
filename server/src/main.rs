use crate::config::ConfigBuilder;
use std::env;

mod certificate;
mod config;
mod connection;
mod error;
mod server;

pub use crate::{config::Config, connection::Connection, error::ServerError};

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

    match server::start(config).await {
        Ok(()) => {}
        Err(err) => log::error!("{err}"),
    }
}
