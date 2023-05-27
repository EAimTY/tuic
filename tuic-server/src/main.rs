use self::{
    config::{Config, ConfigError},
    server::Server,
};
use env_logger::Builder as LoggerBuilder;
use quinn::ConnectionError;
use rustls::Error as RustlsError;
use std::{env, io::Error as IoError, net::SocketAddr, process};
use thiserror::Error;
use tuic::Address;
use tuic_quinn::Error as ModelError;
use uuid::Uuid;

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

    LoggerBuilder::new()
        .filter_level(cfg.log_level)
        .format_module_path(false)
        .format_target(false)
        .init();

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
    Rustls(#[from] RustlsError),
    #[error("invalid max idle time")]
    InvalidMaxIdleTime,
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("duplicated authentication")]
    DuplicatedAuth,
    #[error("token length too short")]
    ExportKeyingMaterial,
    #[error("authentication failed: {0}")]
    AuthFailed(Uuid),
    #[error("received packet from unexpected source")]
    UnexpectedPacketSource,
    #[error("{0}: {1}")]
    Socket(&'static str, IoError),
    #[error("task negotiation timed out")]
    TaskNegotiationTimeout,
    #[error("{0} resolved to {1} but IPv6 UDP relaying is disabled")]
    UdpRelayIpv6Disabled(Address, SocketAddr),
}
