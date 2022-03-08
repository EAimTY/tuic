use crate::{client::TuicClient, config::ConfigBuilder, socks5::Socks5Server};
use std::{env, process};

mod cert;
mod client;
mod config;
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

    let (tuic_client, req_tx) =
        match TuicClient::init(config.server_addr, config.certificate, config.token_digest) {
            Ok((client, tx)) => (tokio::spawn(client.run()), tx),
            Err(err) => {
                eprintln!("{err}");
                process::exit(1);
            }
        };

    let socks5_server =
        match Socks5Server::init(config.local_addr, config.socks5_auth, req_tx).await {
            Ok(server) => tokio::spawn(server.run()),
            Err(err) => {
                eprintln!("{err}");
                process::exit(1);
            }
        };

    let _ = tokio::try_join!(tuic_client, socks5_server);
}
