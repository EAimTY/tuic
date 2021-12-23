use config::{Config, ConfigBuilder};
use std::env;

mod certificate;
mod client;
mod config;
mod server;

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

    if let Some(err) = match config {
        Config::Client(cfg) => client::start(cfg).await.err(),
        Config::Server(cfg) => server::start(cfg).await.err(),
    } {
        eprintln!("{}", err);
    }
}
