use self::{
    config::{Config, ConfigError},
    server::Server,
};
use quinn::{crypto::ExportKeyingMaterialError, ConnectionError};
use std::{env, io::Error as IoError, process};
use thiserror::Error;
use tuic_quinn::Error as ModelError;

mod config;
mod server;
mod utils;

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

    match Server::init(cfg) {
        Ok(server) => server.start().await,
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("duplicated authentication")]
    DuplicatedAuth,
    #[error("token length too short")]
    ExportKeyingMaterial,
    #[error("authentication failed")]
    AuthFailed,
    #[error("received packet from unexpected source")]
    UnexpectedPacketSource,
}
