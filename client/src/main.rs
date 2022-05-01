use crate::{
    config::{Config, ConfigError},
    relay::Relay,
    socks5::Socks5,
};
use std::{env, process};

mod certificate;
mod config;
mod relay;
mod socks5;

#[tokio::main]
async fn main() {
    let args = env::args_os();

    let config = match Config::parse(args) {
        Ok(cfg) => cfg,
        Err(err) => {
            match err {
                ConfigError::Help(help) => println!("{help}"),
                ConfigError::Version(version) => println!("{version}"),
                err => eprintln!("{err}"),
            }
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
        config.client_config,
        config.server_addr,
        config.token_digest,
        config.udp_mode,
        config.heartbeat_interval,
        config.ipv6_endpoint,
        config.reduce_rtt,
    ) {
        Ok((relay, tx)) => (tokio::spawn(relay.run()), tx),
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    let socks5 = match Socks5::init(config.local_addr, config.socks5_auth, req_tx).await {
        Ok(socks5) => tokio::spawn(socks5.run()),
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    let res = tokio::select! {
        res = relay => res,
        res = socks5 => res,
    };

    match res {
        Ok(()) => {}
        Err(err) => eprintln!("{err}"),
    }

    process::exit(1);
}
