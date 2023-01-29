use self::config::{Config, ConfigError};
use std::{env, process};

mod config;
mod connection;
mod error;
mod socks5;

#[tokio::main]
async fn main() {
    let _cfg = match Config::parse(env::args_os()) {
        Ok(cfg) => cfg,
        Err(ConfigError::Version(msg) | ConfigError::Help(msg)) => {
            println!("{msg}");
            process::exit(0);
        }
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    };

    if let Err(err) = socks5::start().await {
        eprintln!("{err}");
        process::exit(1);
    }
}
