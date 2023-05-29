use env_logger::Builder as LoggerBuilder;
use std::{env, process};
use tuic_server::{Config, ConfigError, Server};

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
