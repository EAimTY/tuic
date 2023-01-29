use self::{
    config::{Config, ConfigError},
    connection::Connection,
};
use std::{env, process};

mod config;
mod connection;
mod error;
mod socks5;

#[tokio::main]
async fn main() {
    let cfg = match Config::parse(env::args_os()) {
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
}
